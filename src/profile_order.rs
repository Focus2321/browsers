// Id-based ordering of browser profiles.
// Positions are always resolved on the full list (visible and hidden profiles together), so a
// move never depends on which rows a particular view happens to show.

// the complete order of every profile: `visible` (already sorted the way the popup shows it)
// followed by `hidden`, stably sorted by each id's position in `saved_order`.
// Ids missing from `saved_order` keep their relative order and go after every saved one, which
// matches how sort_browser_profiles places them in the popup.
pub fn full_order(visible: &[String], hidden: &[String], saved_order: &[String]) -> Vec<String> {
    let mut all: Vec<String> = visible.iter().chain(hidden.iter()).cloned().collect();
    all.sort_by_key(|id| {
        saved_order
            .iter()
            .position(|saved| saved == id)
            .unwrap_or(usize::MAX)
    });
    all
}

// keeps each app's profiles together: every profile moves up to where its app's first profile
// sits, keeping its order among them.
// The popup and Settings both show an app's profiles as one group, wherever they were saved.
pub fn group_by_app<T>(items: Vec<T>, app_id: impl Fn(&T) -> String) -> Vec<T> {
    let apps: Vec<String> = items.iter().map(&app_id).collect();
    let mut keyed: Vec<(usize, usize, T)> = items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let first = apps.iter().position(|a| *a == apps[i]).unwrap_or(i);
            (first, i, item)
        })
        .collect();
    keyed.sort_by_key(|(first, i, _)| (*first, *i));
    keyed.into_iter().map(|(_, _, item)| item).collect()
}

// moves every id in `moving` (keeping their relative order) so they sit right before `before`,
// or at the very end when `before` is None.
// Dropping onto one of the moved ids is a no-op.
// Unknown ids in `moving` are ignored; an unknown `before` also means "move to the end".
pub fn move_before(order: &[String], moving: &[String], before: Option<&str>) -> Vec<String> {
    if before.is_some_and(|b| moving.iter().any(|id| id == b)) {
        return order.to_vec();
    }

    let moved: Vec<String> = order
        .iter()
        .filter(|id| moving.contains(id))
        .cloned()
        .collect();
    let mut rest: Vec<String> = order
        .iter()
        .filter(|id| !moving.contains(id))
        .cloned()
        .collect();

    let insert_at = before
        .and_then(|b| rest.iter().position(|id| id == b))
        .unwrap_or(rest.len());
    rest.splice(insert_at..insert_at, moved);
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn full_order_keeps_hidden_profiles_in_their_saved_place() {
        let order = full_order(
            &ids(&["zen", "chrome", "brave"]),
            &ids(&["safari"]),
            &ids(&["zen", "safari", "chrome", "brave"]),
        );
        assert_eq!(order, ids(&["zen", "safari", "chrome", "brave"]));
    }

    #[test]
    fn full_order_puts_unsaved_ids_last_in_their_natural_order() {
        let order = full_order(
            &ids(&["chrome", "new1", "new2"]),
            &ids(&["slack"]),
            &ids(&["chrome"]),
        );
        assert_eq!(order, ids(&["chrome", "new1", "new2", "slack"]));
    }

    #[test]
    fn group_by_app_pulls_profiles_up_to_their_first_one() {
        // a Chrome profile saved above Brave keeps the whole Chrome group in that spot
        let order = ids(&["chrome#work", "brave", "chrome#tf", "safari", "chrome#me"]);
        let grouped = group_by_app(order, |id| id.split('#').next().unwrap().to_string());
        assert_eq!(
            grouped,
            ids(&["chrome#work", "chrome#tf", "chrome#me", "brave", "safari"])
        );
    }

    #[test]
    fn group_by_app_keeps_an_already_grouped_order() {
        let order = ids(&["safari", "chrome#1", "chrome#2", "brave"]);
        let grouped = group_by_app(order.clone(), |id| id.split('#').next().unwrap().to_string());
        assert_eq!(grouped, order);
    }

    #[test]
    fn move_before_moves_down_past_hidden_profiles() {
        // "safari" is hidden and not shown, but the move still lands right before "brave"
        let order = ids(&["zen", "chrome", "safari", "brave"]);
        let moved = move_before(&order, &ids(&["zen"]), Some("brave"));
        assert_eq!(moved, ids(&["chrome", "safari", "zen", "brave"]));
    }

    #[test]
    fn move_before_moves_up() {
        let order = ids(&["zen", "chrome", "safari", "brave"]);
        let moved = move_before(&order, &ids(&["brave"]), Some("zen"));
        assert_eq!(moved, ids(&["brave", "zen", "chrome", "safari"]));
    }

    #[test]
    fn move_before_none_moves_to_end() {
        let order = ids(&["zen", "chrome", "safari"]);
        let moved = move_before(&order, &ids(&["zen"]), None);
        assert_eq!(moved, ids(&["chrome", "safari", "zen"]));
    }

    #[test]
    fn move_before_moves_a_whole_group_together() {
        let order = ids(&["safari", "chrome#1", "chrome#2", "chrome#3", "brave"]);
        let moved = move_before(
            &order,
            &ids(&["chrome#1", "chrome#2", "chrome#3"]),
            Some("safari"),
        );
        assert_eq!(
            moved,
            ids(&["chrome#1", "chrome#2", "chrome#3", "safari", "brave"])
        );
    }

    #[test]
    fn move_before_itself_is_a_no_op() {
        let order = ids(&["zen", "chrome", "safari"]);
        assert_eq!(move_before(&order, &ids(&["chrome"]), Some("chrome")), order);
        // dropping right below itself is also a no-op
        assert_eq!(move_before(&order, &ids(&["chrome"]), Some("safari")), order);
    }

    #[test]
    fn move_before_ignores_unknown_ids() {
        let order = ids(&["zen", "chrome"]);
        assert_eq!(move_before(&order, &ids(&["nope"]), Some("zen")), order);
        assert_eq!(
            move_before(&order, &ids(&["zen"]), Some("nope")),
            ids(&["chrome", "zen"])
        );
    }
}
