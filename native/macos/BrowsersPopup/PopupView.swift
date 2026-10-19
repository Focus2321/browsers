// The popup's SwiftUI content: link header, browser rows and the action bar.
import AppKit
import Combine
import SwiftUI

enum Metrics {
    static let width: CGFloat = 300
    static let cornerRadius: CGFloat = 12
    static let headerHeight: CGFloat = 50
    static let actionBarHeight: CGFloat = 30
    static let rowHeight: CGFloat = 40
    // gap between the panel's edges and the list / selection highlight
    static let listInset: CGFloat = 5
    // left edge of the header and action bar text, lined up with the row icons
    static let textInset: CGFloat = 14
    // most rows shown before the list starts scrolling
    static let maxVisibleRows = 10

    static func visibleRows(_ rowCount: Int) -> Int {
        // at least 1, so an empty filter result still has room for its "No matches" line
        min(max(rowCount, 1), maxVisibleRows)
    }

    static func listHeight(rowCount: Int) -> CGFloat {
        CGFloat(visibleRows(rowCount)) * rowHeight + 2 * listInset
    }

    // header, separator, rows, separator, action bar
    static func panelHeight(rowCount: Int) -> CGFloat {
        headerHeight + 1 + listHeight(rowCount: rowCount) + 1 + actionBarHeight
    }
}

struct PopupView: View {
    @ObservedObject var model: PopupModel
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        VStack(spacing: 0) {
            HeaderView(model: model)
                .frame(height: Metrics.headerHeight)
            Divider()
            RowList(model: model)
                .frame(height: Metrics.listHeight(rowCount: model.rows.count))
            Divider()
            ActionBar(model: model)
                .frame(height: Metrics.actionBarHeight)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        // the hairline edge menus and popovers have, a borderless window doesn't get one.
        // Circular corners, to line up with the vibrancy mask's NSBezierPath.
        .overlay(
            RoundedRectangle(cornerRadius: Metrics.cornerRadius, style: .circular)
                .strokeBorder(
                    colorScheme == .dark ? Color.white.opacity(0.14) : Color.black.opacity(0.08),
                    lineWidth: 1
                )
        )
    }
}

// MARK: - header

// where the link goes: domain first, path second, the app it came from on the side.
// The filter query and the private-window pill take over this spot when active.
private struct HeaderView: View {
    @ObservedObject var model: PopupModel

    var body: some View {
        Group {
            if model.filter.isEmpty && !model.privateMode {
                link
            } else {
                query
            }
        }
        .padding(.horizontal, Metrics.textInset)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture { model.copyLink() }
        .help("Copy Link")
    }

    private var state: PopupState { model.state }

    private var sideNote: String? {
        if model.linkCopied { return "Link copied" }
        return state.sourceAppName.isEmpty ? nil : "from \(state.sourceAppName)"
    }

    private var link: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(verbatim: state.urlDomain.isEmpty ? "No link" : state.urlDomain)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(state.urlDomain.isEmpty ? .secondary : .primary)
                    .lineLimit(1)
                    .layoutPriority(1)
                Spacer(minLength: 0)
                if let sideNote {
                    Text(verbatim: sideNote)
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }
            }
            if !state.urlPath.isEmpty {
                Text(verbatim: state.urlPath)
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
    }

    private var query: some View {
        HStack(spacing: 8) {
            if model.privateMode {
                Text("Private window")
                    .font(.system(size: 11, weight: .semibold))
                    .padding(.horizontal, 8)
                    .frame(height: 20)
                    .background(Capsule().fill(Color.primary.opacity(0.1)))
                    .fixedSize()
            }
            if !model.filter.isEmpty {
                // the caret hugs the end of the query, like a text field's
                HStack(spacing: 1) {
                    Text(verbatim: model.filter)
                        .font(.system(size: 13))
                        .lineLimit(1)
                        .truncationMode(.head)
                    Caret().id(model.filter)
                }
                .layoutPriority(1)
            }
            Spacer(minLength: 0)
            Text(verbatim: model.linkCopied ? "Link copied" : state.urlDomain)
                .font(.system(size: 11))
                .foregroundColor(.secondary)
                .lineLimit(1)
        }
    }
}

// blinks like a text field's insertion point, restarting visible after each edit (see .id above)
private struct Caret: View {
    @State private var visible = true
    private let blink = Timer.publish(every: 0.53, on: .main, in: .common).autoconnect()

    var body: some View {
        RoundedRectangle(cornerRadius: 1)
            .fill(Color.accentColor)
            .frame(width: 2, height: 16)
            .opacity(visible ? 1 : 0)
            .onReceive(blink) { _ in visible.toggle() }
    }
}

// MARK: - rows

private struct RowList: View {
    @ObservedObject var model: PopupModel

    private static let topID = "top"
    private static let bottomID = "bottom"

    var body: some View {
        if model.rows.isEmpty {
            Text(verbatim: model.filter.isEmpty ? "No browsers found" : "No matches for “\(model.filter)”")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .padding(.horizontal, Metrics.textInset)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollViewReader { proxy in
                ScrollView(.vertical) {
                    VStack(spacing: 0) {
                        Color.clear.frame(height: Metrics.listInset).id(Self.topID)
                        ForEach(Array(model.rows.enumerated()), id: \.offset) { position, row in
                            RowView(model: model, row: row, position: position)
                                .id(position)
                        }
                        Color.clear.frame(height: Metrics.listInset).id(Self.bottomID)
                    }
                }
                .modifier(NoBounceWhenFitting())
                .onChange(of: model.scrollRequest) { request in
                    scroll(proxy, to: request)
                }
            }
        }
    }

    private func scroll(_ proxy: ScrollViewProxy, to request: ScrollRequest) {
        // the first and last rows bring their bit of inset along, so they don't end up flush
        // against the separator
        if request.position <= 0 {
            proxy.scrollTo(Self.topID, anchor: request.anchor)
        } else if request.position >= model.rows.count - 1 {
            proxy.scrollTo(Self.bottomID, anchor: request.anchor)
        } else {
            proxy.scrollTo(request.position, anchor: request.anchor)
        }
    }
}

// up to 10 rows the list fits exactly and shouldn't rubber-band on a trackpad swipe
private struct NoBounceWhenFitting: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 13.3, *) {
            content.scrollBounceBehavior(.basedOnSize)
        } else {
            content
        }
    }
}

private struct RowView: View {
    @ObservedObject var model: PopupModel
    let row: BrowserEntry
    let position: Int

    private var isSelected: Bool { model.selection == position }
    // rows that can't open a private window fade out while ⇧ is held
    private var privateUnavailable: Bool { model.privateMode && !row.supportsIncognito }
    // holding ⇧ turns the selection neutral, so it doesn't look like a regular open is about to
    // happen
    private var accentSelected: Bool { isSelected && !model.privateMode }

    var body: some View {
        HStack(spacing: 10) {
            AvatarView(row: row)
            VStack(alignment: .leading, spacing: 0) {
                Text(verbatim: row.title)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(accentSelected ? .white : .primary)
                    .lineLimit(1)
                if row.supportsProfiles {
                    Text(verbatim: row.browserName)
                        .font(.system(size: 11))
                        .foregroundColor(accentSelected ? .white.opacity(0.8) : .secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            trailingKey
        }
        .padding(.leading, 8)
        .padding(.trailing, 7)
        .frame(height: Metrics.rowHeight)
        .background(selectionBackground)
        .padding(.horizontal, Metrics.listInset)
        .opacity(privateUnavailable ? 0.35 : 1)
        .contentShape(Rectangle())
        .onContinuousHover { phase in
            if case .active = phase { model.hover(position) }
        }
        .onTapGesture { model.click(position) }
        .contextMenu { contextMenu }
    }

    @ViewBuilder private var selectionBackground: some View {
        if isSelected {
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(model.privateMode
                    ? Color.primary.opacity(0.1)
                    : Color.accentColor)
        }
    }

    // ↩ on the selected row says what Return does, the other rows show their 1-9 hotkey
    @ViewBuilder private var trailingKey: some View {
        if isSelected && !privateUnavailable {
            KeyCap(label: model.privateMode ? "⇧↩" : "↩", onAccent: accentSelected)
        } else if model.state.showHotkeys && position < 9 {
            KeyCap(label: "\(position + 1)", onAccent: accentSelected)
        }
    }

    @ViewBuilder private var contextMenu: some View {
        let domain = model.state.urlDomain
        if row.supportsIncognito {
            Button("Open in Private Window") { model.open(position, privately: true) }
        }
        Button("Copy Link") { model.copyLink() }
        if !domain.isEmpty {
            Button("Always open \(domain) here" as String) { model.alwaysOpenHere(row) }
        }
        Divider()
        Button("Hide \(row.fullName)" as String) {
            model.send("hide_profile", ["unique_id": row.uniqueId])
        }
        if row.supportsProfiles {
            Button("Hide All \(row.browserName) Profiles" as String) {
                model.send("hide_app", ["unique_app_id": row.uniqueAppId])
            }
        }
        Divider()
        Button("Manage Browsers…") { model.send("settings") }
    }
}

private struct KeyCap: View {
    let label: String
    let onAccent: Bool

    var body: some View {
        Text(verbatim: label)
            .font(.system(size: 11, weight: .medium))
            .foregroundColor(onAccent ? .white : .secondary)
            .padding(.horizontal, 5)
            .frame(minWidth: 20, minHeight: 18)
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(onAccent ? Color.white.opacity(0.2) : Color.primary.opacity(0.07))
            )
    }
}

// the profile picture (or the app icon when the app has no profiles), with a small app icon
// badge in the corner for profile rows, so five Chrome profiles don't turn into five identical
// Chrome icons
private struct AvatarView: View {
    let row: BrowserEntry

    var body: some View {
        ZStack(alignment: .topLeading) {
            if row.supportsProfiles {
                profilePicture
                    .frame(width: 24, height: 24)
                    .offset(x: 1, y: 1)
                IconImage(path: row.iconPath)
                    .frame(width: 16, height: 16)
                    .shadow(color: .black.opacity(0.3), radius: 1, y: 0.5)
                    .offset(x: 14, y: 14)
            } else {
                IconImage(path: row.iconPath)
            }
        }
        .frame(width: 28, height: 28, alignment: .topLeading)
    }

    @ViewBuilder private var profilePicture: some View {
        if let image = IconCache.image(row.profileIconPath) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: .fill)
                .clipShape(Circle())
        } else {
            Circle()
                .fill(Color(hex: row.avatarColor) ?? .gray)
                .overlay(
                    Text(verbatim: row.profileInitial)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundColor(.white)
                )
        }
    }
}

private struct IconImage: View {
    let path: String

    var body: some View {
        if let image = IconCache.image(path) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: .fit)
        } else {
            Image(nsImage: NSWorkspace.shared.icon(for: .applicationBundle))
                .resizable()
                .aspectRatio(contentMode: .fit)
        }
    }
}

// MARK: - action bar

// what the keyboard can do, plus the "···" options menu
private struct ActionBar: View {
    @ObservedObject var model: PopupModel

    var body: some View {
        HStack(spacing: 12) {
            Hint(keys: "↩", label: "Open")
            Hint(keys: "⇧↩", label: "Private")
            Hint(keys: "⌘C", label: "Copy")
            Spacer(minLength: 0)
            OptionsButton(model: model)
        }
        .padding(.leading, Metrics.textInset)
        .padding(.trailing, 6)
    }
}

private struct Hint: View {
    let keys: String
    let label: String

    var body: some View {
        HStack(spacing: 4) {
            Text(verbatim: keys)
                .font(.system(size: 11, weight: .semibold))
                .foregroundColor(.secondary)
            Text(verbatim: label)
                .font(.system(size: 11))
                .foregroundColor(.secondary)
        }
    }
}

private struct OptionsButton: View {
    @ObservedObject var model: PopupModel
    @State private var hovering = false

    var body: some View {
        GeometryReader { geometry in
            Button {
                model.showOptionsMenu(anchor: geometry.frame(in: .global))
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(.secondary)
                    .frame(width: geometry.size.width, height: geometry.size.height)
                    .background(
                        RoundedRectangle(cornerRadius: 5, style: .continuous)
                            .fill(Color.primary.opacity(hovering ? 0.1 : 0))
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        .frame(width: 28, height: 20)
        .onHover { hovering = $0 }
        .help("More")
    }
}

// MARK: - helpers

// app and profile icons are small PNGs that rarely change, so each is read from disk once
@MainActor
enum IconCache {
    private static var images: [String: NSImage] = [:]
    private static var missing: Set<String> = []

    static func image(_ path: String) -> NSImage? {
        if path.isEmpty || missing.contains(path) { return nil }
        if let image = images[path] { return image }
        guard let image = NSImage(contentsOfFile: path) else {
            missing.insert(path)
            return nil
        }
        images[path] = image
        return image
    }
}

private extension Color {
    // "#7c5ce6"
    init?(hex: String) {
        let digits = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
        guard digits.count == 6, let value = UInt32(digits, radix: 16) else { return nil }
        self.init(
            .sRGB,
            red: Double((value >> 16) & 0xFF) / 255,
            green: Double((value >> 8) & 0xFF) / 255,
            blue: Double(value & 0xFF) / 255
        )
    }
}
