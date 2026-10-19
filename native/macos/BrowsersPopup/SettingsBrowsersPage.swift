// The Browsers page: the apps and profiles in popup order, with hiding, grouping, reordering, and
// the "not web browsers" banner.
import AppKit
import SwiftUI

struct SettingsBrowsersPage: View {
    @ObservedObject var model: SettingsModel

    private var browsers: SettingsBrowsersState { model.state.browsers }

    var body: some View {
        Form {
            if let banner = browsers.banner {
                Section {
                } header: {
                    SettingsBannerView(banner: banner, model: model)
                }
            }

            if browsers.rows.isEmpty && browsers.specialRows.isEmpty {
                Section {
                    Text(verbatim: browsers.emptyText)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 60)
                } footer: {
                    footer
                }
            } else {
                Section {
                    ForEach(Array(browsers.rows.enumerated()), id: \.element.id) { index, row in
                        SettingsBrowserRow(model: model, list: .rows, index: index, row: row)
                    }
                } header: {
                    HStack(alignment: .firstTextBaseline) {
                        // a header that isn't a plain Text loses the header style, so it's restated
                        Text("Popup order")
                            .font(.headline)
                            .foregroundStyle(.primary)
                        Spacer()
                        Text(verbatim: browsers.orderHint)
                            .font(.caption)
                            .fontWeight(.regular)
                            .foregroundStyle(.secondary)
                    }
                } footer: {
                    if browsers.specialRows.isEmpty { footer }
                }

                if !browsers.specialRows.isEmpty {
                    Section {
                        ForEach(Array(browsers.specialRows.enumerated()), id: \.element.id) { index, row in
                            SettingsBrowserRow(model: model, list: .specialRows, index: index, row: row)
                        }
                    } header: {
                        Text("Only for their own links · Always listed first")
                    } footer: {
                        footer
                    }
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Browsers")
        .searchable(
            text: Binding(get: { model.filter }, set: { model.setFilter($0) }),
            placement: .toolbar,
            prompt: Text("Filter")
        )
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Toggle(isOn: Binding(get: { browsers.showHidden }, set: { model.setShowHidden($0) })) {
                    Label {
                        Text("Show hidden")
                    } icon: {
                        Image(systemName: browsers.showHidden ? "eye.fill" : "eye")
                    }
                }
                .help(browsers.hiddenCount > 0 ? "Show hidden (\(browsers.hiddenCount))" : "Show hidden")
                .overlay(alignment: .topTrailing) {
                    if browsers.hiddenCount > 0 {
                        SettingsToolbarBadge(count: browsers.hiddenCount)
                    }
                }

                Button {
                    model.rescan()
                } label: {
                    Label("Rescan", systemImage: "arrow.clockwise")
                }
                .help("Rescan")
            }
        }
    }

    private var footer: some View {
        HStack(alignment: .firstTextBaseline) {
            Button {
            } label: {
                Label("Add an app…", systemImage: "plus")
            }
            .buttonStyle(.link)
            .disabled(true)
            .help("Coming soon - apps that can open links are found automatically")
            Spacer()
            Text(verbatim: browsers.foundText)
                .foregroundStyle(.secondary)
        }
        .font(.callout)
        .padding(.top, 4)
    }
}

// the hidden count on "Show hidden", like a toolbar item badge
private struct SettingsToolbarBadge: View {
    let count: Int

    var body: some View {
        Text(verbatim: "\(count)")
            .font(.system(size: 9, weight: .semibold).monospacedDigit())
            .foregroundColor(.white)
            .padding(.horizontal, 4)
            .frame(minWidth: 14, minHeight: 14)
            .background(Capsule().fill(Color.red))
            .offset(x: 6, y: -5)
            .allowsHitTesting(false)
    }
}

private struct SettingsBannerView: View {
    let banner: SettingsBanner
    @ObservedObject var model: SettingsModel

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "info.circle.fill")
                .font(.title2)
                .foregroundStyle(Color.accentColor)
            VStack(alignment: .leading, spacing: 3) {
                Text(verbatim: banner.title)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.primary)
                Text(verbatim: banner.message)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 8) {
                    Button("Hide them") { model.hideNonBrowsers() }
                        .buttonStyle(.borderedProminent)
                    Button("Not now") { model.dismissBanner() }
                }
                .controlSize(.small)
                .padding(.top, 5)
            }
            Spacer(minLength: 0)
        }
        .textCase(nil)
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color.accentColor.opacity(0.1))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Color.accentColor.opacity(0.25), lineWidth: 1)
        )
        .padding(.bottom, 4)
    }
}

// one app, group or profile: avatar, name over detail, status, eye, "···", drag handle
private struct SettingsBrowserRow: View {
    @ObservedObject var model: SettingsModel
    let list: SettingsRowList
    let index: Int
    let row: SettingsRow

    @State private var height: CGFloat = 40

    private var draggable: Bool { list == .rows && row.draggable }

    private var insertionEdge: SettingsInsertionLine.Edge? {
        guard list == .rows, let drag = model.drag, drag.kind == .browserRow,
              let insertion = drag.insertion
        else { return nil }
        if insertion == index { return .top }
        if insertion == index + 1 && index == model.state.browsers.rows.count - 1 { return .bottom }
        return nil
    }

    var body: some View {
        HStack(spacing: 10) {
            if list == .rows {
                SettingsDragHandle()
                    .opacity(draggable ? 1 : 0)
            }
            disclosure
                .padding(.leading, row.kind == .child ? 24 : 0)
            SettingsAvatar(
                iconPath: row.iconPath, letter: row.letter, color: row.letterColor,
                size: row.kind == .child ? 22 : 26, circular: row.kind == .child
            )
            .opacity(row.hidden ? 0.45 : 1)
            .frame(width: 26)

            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: row.name)
                    .fontWeight(row.kind == .header ? .medium : .regular)
                    .foregroundStyle(row.hidden ? .secondary : .primary)
                    .lineLimit(1)
                if !row.detail.isEmpty {
                    Text(verbatim: row.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .layoutPriority(1)

            Spacer(minLength: 8)

            HStack(spacing: 6) {
                if row.isDefault { SettingsStatusTag(text: "Default profile", accent: true) }
                if row.hidden { SettingsStatusTag(text: "Hidden", accent: false) }
            }

            HStack(spacing: 12) {
                Button {
                    model.toggleVisibility(list, index)
                } label: {
                    Image(systemName: row.hidden ? "eye.slash" : "eye")
                        .frame(width: 20)
                }
                .buttonStyle(.borderless)
                .foregroundStyle(row.hidden ? .tertiary : .secondary)
                .help(row.hidden ? "Show in the popup" : "Hide from the popup")
                .accessibilityLabel(row.hidden ? "Show in the popup" : "Hide from the popup")

                if row.menu.isEmpty {
                    Color.clear.frame(width: 16, height: 1)
                } else {
                    SettingsMoreMenu {
                        ForEach(row.menu, id: \.action) { item in
                            Button(item.title) { model.rowMenu(list, index, item.action) }
                        }
                    }
                    .foregroundStyle(.secondary)
                }

            }
        }
        .frame(minHeight: 30)
        .contentShape(Rectangle())
        .modifier(SettingsHeightReader(height: $height))
        .overlay {
            if let edge = insertionEdge { SettingsInsertionLine(edge: edge) }
        }
        .modifier(BrowserRowDrag(model: model, list: list, index: index, row: row, draggable: draggable, height: height))
    }

    // the group's chevron; apps without profiles keep the space, so every icon lines up
    @ViewBuilder private var disclosure: some View {
        if row.kind == .header {
            Button {
                model.setCollapsed(row, !row.collapsed)
            } label: {
                Image(systemName: "chevron.right")
                    .font(.system(size: 11, weight: .semibold))
                    .rotationEffect(.degrees(row.collapsed ? 0 : 90))
                    .frame(width: 14, height: 20)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .help(row.collapsed ? "Show profiles" : "Hide profiles")
            .accessibilityLabel(row.collapsed ? "Expand \(row.name)" : "Collapse \(row.name)")
        } else {
            Color.clear.frame(width: 14, height: 1)
        }
    }
}

private struct BrowserRowDrag: ViewModifier {
    let model: SettingsModel
    let list: SettingsRowList
    let index: Int
    let row: SettingsRow
    let draggable: Bool
    let height: CGFloat

    func body(content: Content) -> some View {
        if list == .rows {
            let dropTarget = content.onDrop(
                of: [.plainText],
                delegate: SettingsRowDropDelegate(model: model, kind: .browserRow, index: index, height: height)
            )
            if draggable {
                dropTarget.onDrag {
                    model.beginDrag(.browserRow, from: index, id: row.id)
                    return NSItemProvider(object: row.id as NSString)
                }
            } else {
                dropTarget
            }
        } else {
            content
        }
    }
}

// "Default profile" / "Hidden" after a row's name
private struct SettingsStatusTag: View {
    let text: String
    let accent: Bool

    var body: some View {
        Text(verbatim: text)
            .font(.caption)
            .foregroundStyle(accent ? Color.accentColor : Color.secondary)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(
                Capsule().fill(accent ? Color.accentColor.opacity(0.14) : Color.primary.opacity(0.07))
            )
            .fixedSize()
    }
}
