// The General page: theme, popup behavior, running in the background.
// Every control shows the state's value and only sends the new one, the update that answers the
// event is what changes it.
import SwiftUI

struct SettingsGeneralPage: View {
    @ObservedObject var model: SettingsModel

    private var general: SettingsGeneralState { model.state.general }

    var body: some View {
        Form {
            Section("Appearance") {
                Picker("Theme", selection: Binding(get: { general.theme }, set: { theme in
                    if theme != general.theme { model.setTheme(theme) }
                })) {
                    Text("Auto").tag(Appearance.system)
                    Text("Light").tag(Appearance.light)
                    Text("Dark").tag(Appearance.dark)
                }
                .pickerStyle(.segmented)
            }

            Section("Popup") {
                if general.nativePopupAvailable {
                    SettingsSwitchRow(
                        title: "Native popup",
                        help: "Use the macOS look for the link picker.",
                        isOn: general.nativePopup
                    ) { model.setNativePopup($0) }
                }
                SettingsSwitchRow(
                    title: "Show hotkey numbers",
                    help: "Press 1–9 in the popup to open the matching app.",
                    isOn: general.showHotkeys
                ) { model.setShowHotkeys($0) }
                if general.quitOnLostFocusVisible {
                    SettingsSwitchRow(
                        title: "Close when clicking elsewhere",
                        help: "The popup goes away as soon as another window takes focus.",
                        isOn: general.quitOnLostFocus
                    ) { model.setQuitOnLostFocus($0) }
                }
                SettingsSwitchRow(
                    title: "Unwrap tracking links",
                    help: "Opens the real destination of Outlook Safe Links and Messenger links.",
                    isOn: general.unwrapUrls
                ) { model.setUnwrapUrls($0) }
            }

            Section("Performance") {
                SettingsSwitchRow(
                    title: "Keep running in the background",
                    help: "Opens links instantly. Uses a little memory while idle.",
                    isOn: general.runInBackground
                ) { model.setRunInBackground($0) }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("General")
        .modifier(SettingsKeepToolbar())
    }
}

// a title over a secondary helper line, the switch on the right
private struct SettingsSwitchRow: View {
    let title: String
    let help: String
    let isOn: Bool
    let onSet: (Bool) -> Void

    var body: some View {
        Toggle(isOn: Binding(get: { isOn }, set: { value in
            if value != isOn { onSet(value) }
        })) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                Text(help)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .toggleStyle(.switch)
    }
}
