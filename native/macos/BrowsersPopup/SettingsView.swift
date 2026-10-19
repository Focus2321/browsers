// The Settings window's SwiftUI content: a System Settings style split view, the page list in the
// sidebar with the default-browser card under it, the selected page on the right.
import AppKit
import SwiftUI

struct SettingsRootView: View {
    @ObservedObject var model: SettingsModel

    var body: some View {
        NavigationSplitView(columnVisibility: .constant(.all)) {
            SettingsSidebar(model: model)
                .frame(minWidth: 200)
                .navigationSplitViewColumnWidth(min: 200, ideal: 200, max: 200)
                .modifier(NoSidebarToggle())
        } detail: {
            SettingsDetail(model: model)
        }
        .frame(minWidth: SettingsController.minimumSize.width, minHeight: SettingsController.minimumSize.height)
        // a fresh open starts over: scroll positions, focus, per-row editing state
        .id(model.session)
        .environment(\.openURL, OpenURLAction { url in
            NSWorkspace.shared.open(url)
            return .handled
        })
    }
}

// the sidebar can't be collapsed away
private struct NoSidebarToggle: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 14.0, *) {
            content.toolbar(removing: .sidebarToggle)
        } else {
            content
        }
    }
}

private struct SettingsSidebar: View {
    @ObservedObject var model: SettingsModel

    private var selection: Binding<SettingsPage?> {
        Binding(get: { model.page }, set: { if let page = $0 { model.select(page) } })
    }

    var body: some View {
        List(selection: selection) {
            ForEach(SettingsPage.allCases) { page in
                Label(page.title, systemImage: page.symbol)
                    .badge(page == .rules ? model.state.rules.rules.count : 0)
                    .tag(page)
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            DefaultBrowserCard(model: model)
                .padding(10)
        }
    }
}

private struct DefaultBrowserCard: View {
    @ObservedObject var model: SettingsModel

    private var isDefault: Bool { model.state.isDefaultBrowser }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Circle()
                    .fill(isDefault ? Color.green : Color.orange)
                    .frame(width: 8, height: 8)
                Text(isDefault ? "Default browser" : "Not the default browser")
                    .font(.callout.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.9)
            }
            Text(isDefault ? "Links from every app come here." : "Links skip Browsers for now.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if !isDefault {
                Button {
                    model.setDefaultBrowser()
                } label: {
                    Text("Make default").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .padding(.top, 2)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color(nsColor: .quaternaryLabelColor).opacity(0.35))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Color(nsColor: .separatorColor), lineWidth: 0.5)
        )
    }
}

private struct SettingsDetail: View {
    @ObservedObject var model: SettingsModel

    var body: some View {
        switch model.page {
        case .browsers: SettingsBrowsersPage(model: model)
        case .rules: SettingsRulesPage(model: model)
        case .general: SettingsGeneralPage(model: model)
        case .about: SettingsAboutPage(about: model.state.about)
        }
    }
}

// MARK: shared page pieces

// SwiftUI drops the window's toolbar on a page without toolbar items, and the title bar shrinks
// and the page title moves with it. An invisible item keeps it, like System Settings, where every
// page has the toolbar.
struct SettingsKeepToolbar: ViewModifier {
    func body(content: Content) -> some View {
        content.toolbar { SettingsToolbarPlaceholderItem() }
    }
}

private struct SettingsToolbarPlaceholderItem: ToolbarContent {
    var body: some ToolbarContent {
        if #available(macOS 26.0, *) {
            // no Liquid Glass capsule around nothing
            ToolbarItem(placement: .primaryAction) { SettingsToolbarPlaceholder() }
                .sharedBackgroundVisibility(.hidden)
        } else {
            ToolbarItem(placement: .primaryAction) { SettingsToolbarPlaceholder() }
        }
    }
}

private struct SettingsToolbarPlaceholder: View {
    var body: some View {
        Color.clear
            .frame(width: 1, height: 1)
            .accessibilityHidden(true)
    }
}

// a "···" menu: a borderless icon that opens a menu, no indicator
struct SettingsMoreMenu<Content: View>: View {
    var help = "More"
    @ViewBuilder let content: () -> Content

    var body: some View {
        Menu(content: content) {
            Image(systemName: "ellipsis")
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help(help)
        .accessibilityLabel(help)
    }
}

// A pop-up button over options whose label is precomputed by Rust: the current id may not be
// among the options ("Missing app"), or there's nothing picked yet ("Choose…").
struct SettingsOptionPicker: View {
    let options: [SettingsOption]
    // nil = nothing picked
    let selection: String?
    let label: String
    var iconPath = ""
    var placeholder = "Choose…"
    var showIcons = true
    let onPick: (String) -> Void

    private var current: String { selection.map { "id:" + $0 } ?? "none" }

    private var binding: Binding<String> {
        Binding(get: { current }, set: { tag in
            guard tag.hasPrefix("id:") else { return }
            let id = String(tag.dropFirst(3))
            if id != selection { onPick(id) }
        })
    }

    var body: some View {
        Picker(selection: binding) {
            if selection == nil {
                Text(verbatim: placeholder).tag("none")
                Divider()
            } else if !options.contains(where: { $0.id == selection }) {
                // what Rust says it is, e.g. "Missing app"
                item(label, iconPath: iconPath).tag(current)
                Divider()
            }
            ForEach(options, id: \.id) { option in
                item(option.label, iconPath: option.iconPath).tag("id:" + option.id)
            }
        } label: {
            Text(verbatim: label)
        }
        .pickerStyle(.menu)
        .labelsHidden()
    }

    @ViewBuilder private func item(_ text: String, iconPath: String) -> some View {
        if showIcons, let image = SettingsMenuIcon.image(iconPath) {
            Label {
                Text(verbatim: text)
            } icon: {
                Image(nsImage: image)
            }
        } else {
            Text(verbatim: text)
        }
    }
}

// the grip a draggable row shows on its leading edge
struct SettingsDragHandle: View {
    static let width: CGFloat = 12

    var body: some View {
        Image(systemName: "line.3.horizontal")
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(.tertiary)
            .frame(width: Self.width)
            .help("Drag to reorder")
            .accessibilityHidden(true)
    }
}

// the "Private" toggle of a rule or "Everything else"
struct SettingsPrivateToggle: View {
    let isOn: Bool
    let onSet: (Bool) -> Void

    var body: some View {
        Toggle("Private", isOn: Binding(get: { isOn }, set: { onSet($0) }))
            .toggleStyle(.checkbox)
            .fixedSize()
            .help("Open in a private window")
    }
}

// the accent-tinted highlight of the matched rule / "Everything else", drawn into the row's own
// padding (grouped forms don't take row backgrounds)
struct SettingsHighlight: ViewModifier {
    let active: Bool
    var dashed = false

    func body(content: Content) -> some View {
        content.background(
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .fill(Color.accentColor.opacity(active && !dashed ? 0.12 : 0))
                .overlay(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .strokeBorder(
                            Color.accentColor.opacity(active ? (dashed ? 0.7 : 0.55) : 0),
                            style: StrokeStyle(lineWidth: 1, dash: dashed ? [4, 3] : [])
                        )
                )
                .padding(.horizontal, -6)
                .padding(.vertical, -4)
        )
    }
}

// the insertion line of a drag, at the top or bottom edge of a row
struct SettingsInsertionLine: View {
    enum Edge { case top, bottom }

    let edge: Edge

    var body: some View {
        VStack(spacing: 0) {
            if edge == .bottom { Spacer(minLength: 0) }
            HStack(spacing: 0) {
                Circle()
                    .strokeBorder(Color.accentColor, lineWidth: 2)
                    .frame(width: 7, height: 7)
                Rectangle()
                    .fill(Color.accentColor)
                    .frame(height: 2)
            }
            .frame(height: 7)
            .offset(y: edge == .top ? -9 : 9)
            if edge == .top { Spacer(minLength: 0) }
        }
        .allowsHitTesting(false)
    }
}

// a row's drop target: tells the model which half of the row the drag is over
struct SettingsRowDropDelegate: DropDelegate {
    let model: SettingsModel
    let kind: SettingsDrag.Kind
    let index: Int
    let height: CGFloat

    func validateDrop(info: DropInfo) -> Bool { model.drag?.kind == kind }

    func dropEntered(info: DropInfo) { hover(info) }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        guard model.drag?.kind == kind else { return nil }
        hover(info)
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) { model.dragLeaves(kind) }

    func performDrop(info: DropInfo) -> Bool {
        hover(info)
        return model.drop(kind)
    }

    private func hover(_ info: DropInfo) {
        model.dragHovers(kind, row: index, lowerHalf: info.location.y > height / 2)
    }
}

// measures a row, for the drop delegate's halves
struct SettingsHeightReader: ViewModifier {
    @Binding var height: CGFloat

    func body(content: Content) -> some View {
        content.background(
            GeometryReader { geometry in
                Color.clear
                    .onAppear { height = geometry.size.height }
                    .onChange(of: geometry.size.height) { height = $0 }
            }
        )
    }
}
