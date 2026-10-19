// The About content, twice: as the Settings' About page (a grouped form) and as the small
// standalone About window (centered, like the standard macOS About panel).
import AppKit
import SwiftUI

enum AboutMetrics {
    static let iconSize: CGFloat = 64
    static let windowWidth: CGFloat = 320
}

// MARK: Settings page

struct SettingsAboutPage: View {
    let about: AboutState

    var body: some View {
        Form {
            Section {
                VStack(spacing: 6) {
                    SettingsAppIcon(path: about.appIconPath, size: AboutMetrics.iconSize)
                        .padding(.bottom, 4)
                    Text(verbatim: about.appName)
                        .font(.title2.weight(.bold))
                        .textSelection(.enabled)
                    if !about.versionText.isEmpty {
                        Text(verbatim: about.versionText)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                    if !about.copyright.isEmpty {
                        Text(verbatim: about.copyright)
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                    AboutWebsiteLine(url: about.websiteUrl)
                        .font(.callout)
                        .padding(.top, 2)
                }
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 10)
            }

            if !about.directories.isEmpty {
                Section("Folders") {
                    ForEach(Array(about.directories.enumerated()), id: \.offset) { _, directory in
                        LabeledContent {
                            Text(verbatim: directory.path)
                                .font(.callout)
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                                .multilineTextAlignment(.trailing)
                                .lineLimit(2)
                                .truncationMode(.middle)
                        } label: {
                            Text(verbatim: directory.label)
                        }
                    }
                }
            }

            Section {
                AboutAttributionLink(attribution: about.madeWithSlint)
                    .frame(maxWidth: .infinity)
            }
        }
        .formStyle(.grouped)
        .navigationTitle("About")
        .modifier(SettingsKeepToolbar())
    }
}

// MARK: About window

struct AboutWindowView: View {
    @ObservedObject var model: AboutModel

    private var about: AboutState { model.state }

    var body: some View {
        VStack(spacing: 0) {
            SettingsAppIcon(path: about.appIconPath, size: AboutMetrics.iconSize)
                .padding(.bottom, 12)
            Text(verbatim: about.appName)
                .font(.system(size: 14, weight: .bold))
                .textSelection(.enabled)
                .padding(.bottom, 4)
            if !about.versionText.isEmpty {
                Text(verbatim: about.versionText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            if !about.copyright.isEmpty {
                Text(verbatim: about.copyright)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .padding(.top, 2)
            }
            AboutWebsiteLine(url: about.websiteUrl)
                .font(.caption)
                .padding(.top, 10)

            if !about.directories.isEmpty {
                Grid(alignment: .leading, horizontalSpacing: 6, verticalSpacing: 3) {
                    ForEach(Array(about.directories.enumerated()), id: \.offset) { _, directory in
                        GridRow(alignment: .firstTextBaseline) {
                            Text(verbatim: directory.label)
                                .fontWeight(.medium)
                                .foregroundStyle(.secondary)
                                .gridColumnAlignment(.trailing)
                            Text(verbatim: directory.path)
                                .foregroundStyle(.secondary)
                                .multilineTextAlignment(.leading)
                                .textSelection(.enabled)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                .font(.system(size: 10))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 14)
            }

            AboutAttributionLink(attribution: about.madeWithSlint, compact: true)
                .padding(.top, 14)
        }
        .multilineTextAlignment(.center)
        .padding(.horizontal, 20)
        .padding(.top, 24)
        .padding(.bottom, 18)
        .frame(width: AboutMetrics.windowWidth)
        .fixedSize(horizontal: false, vertical: true)
        .environment(\.openURL, OpenURLAction { url in
            NSWorkspace.shared.open(url)
            return .handled
        })
    }
}

// MARK: pieces

// "Visit us at browsers.software." with the address as a link
private struct AboutWebsiteLine: View {
    let url: String

    var body: some View {
        if let link = URL(string: url), !url.isEmpty {
            (Text("Visit us at ") + Text(.init("[\(Self.display(url))](\(link.absoluteString))")) + Text(verbatim: "."))
                .tint(.accentColor)
        }
    }

    // the address without its scheme, like a browser's address bar shows it
    static func display(_ url: String) -> String {
        var text = url
        for scheme in ["https://", "http://"] where text.hasPrefix(scheme) {
            text.removeFirst(scheme.count)
        }
        if text.hasSuffix("/") { text.removeLast() }
        // markdown would read these as syntax
        return text.replacingOccurrences(of: "[", with: "").replacingOccurrences(of: "]", with: "")
    }
}

// the attribution Slint's royalty-free license asks for while the app links Slint: never dropped
private struct AboutAttributionLink: View {
    let attribution: AboutAttribution
    var compact = false

    var body: some View {
        Button {
            openSettingsLink(attribution.url)
        } label: {
            VStack(spacing: 1) {
                Text(verbatim: attribution.label)
                    .foregroundStyle(Color.accentColor)
                if !attribution.versionText.isEmpty {
                    Text(verbatim: attribution.versionText)
                        .font(compact ? .system(size: 10) : .caption)
                        .foregroundStyle(.secondary)
                }
            }
            .font(compact ? .caption : .callout)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(attribution.url)
        .accessibilityAddTraits(.isLink)
        .onHover { inside in
            if inside { NSCursor.pointingHand.push() } else { NSCursor.pop() }
        }
    }
}
