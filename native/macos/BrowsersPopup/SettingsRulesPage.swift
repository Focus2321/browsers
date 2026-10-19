// The Rules page: test a link, the rules checked top to bottom, the new rule being put together,
// and where everything else goes.
import AppKit
import SwiftUI

struct SettingsRulesPage: View {
    @ObservedObject var model: SettingsModel

    private var rules: SettingsRulesState { model.state.rules }

    var body: some View {
        ScrollViewReader { proxy in
            form
                // a new rule shows up at the end of the list, which may be out of sight
                .onChange(of: model.draftFocusRequest) { _ in
                    DispatchQueue.main.async {
                        withAnimation { proxy.scrollTo(SettingsDraftRow.scrollID, anchor: .center) }
                    }
                }
        }
        .navigationTitle("Rules")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    model.addRule()
                } label: {
                    Label("New rule", systemImage: "plus")
                        .labelStyle(.titleAndIcon)
                }
                .buttonStyle(.borderedProminent)
                .help("New rule")
            }
        }
    }

    private var form: some View {
        Form {
            Section("Test a link") {
                SettingsTestLink(model: model)
            }

            Section {
                if !rules.rules.isEmpty {
                    SettingsReorderStack(
                        contents: rules.rules.map { "\($0.pattern)|\($0.sourceId)|\($0.targetId)|\($0.isPrivate)" },
                        levels: rules.rules.map { _ in .top },
                        draggable: rules.rules.map { _ in true },
                        onMove: { model.moveRule($0, to: $1) }
                    ) { index, grip in
                        SettingsRuleRow(model: model, index: index, rule: rules.rules[index], grip: grip)
                    }
                    .listRowInsets(EdgeInsets())
                }
                if let draft = rules.draft {
                    SettingsDraftRow(model: model, draft: draft)
                        .id(SettingsDraftRow.scrollID)
                }
            } header: {
                Text("Checked top to bottom · First match wins")
            }

            Section {
                SettingsFallbackRow(model: model, fallback: rules.fallback)
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: test a link

private struct SettingsTestLink: View {
    @ObservedObject var model: SettingsModel

    private var test: SettingsTest { model.state.rules.test }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                TextField(
                    "Test link",
                    text: Binding(get: { model.testURL }, set: { model.setTestURL($0) }),
                    prompt: Text(verbatim: "Paste a link, like https://github.com/acme/api")
                )
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                Text("from")
                    .foregroundStyle(.secondary)
                SettingsOptionPicker(
                    options: model.state.rules.sourceOptions,
                    selection: test.sourceId,
                    label: test.sourceLabel,
                    showIcons: false
                ) { model.pickTestSource($0) }
                .fixedSize()
            }
            if test.resultKind == .none || test.resultText.isEmpty {
                Text("See which rule a link would use before you click it.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            } else {
                HStack(spacing: 6) {
                    Circle()
                        .fill(dotColor)
                        .frame(width: 7, height: 7)
                    Text(verbatim: test.resultText)
                        .font(.callout)
                        .foregroundStyle(test.resultKind == .invalid ? Color.red : Color.primary)
                        .textSelection(.enabled)
                }
            }
        }
        .padding(.vertical, 2)
    }

    private var dotColor: Color {
        switch test.resultKind {
        case .rule: return .accentColor
        case .invalid: return .red
        case .fallback, .none: return Color(nsColor: .secondaryLabelColor)
        }
    }
}

// MARK: rules

private enum RuleMetrics {
    static let numberWidth: CGFloat = 18
    static let spacing: CGFloat = 8
    // where a rule's second line starts: under the pattern field
    static let indent = SettingsDragHandle.width + spacing + numberWidth
}

private struct SettingsRuleNumber: View {
    let number: Int
    let accent: Bool

    var body: some View {
        Text(verbatim: "\(number)")
            .font(.body.monospacedDigit().weight(accent ? .semibold : .regular))
            .foregroundStyle(accent ? Color.accentColor : Color.secondary)
            .frame(width: RuleMetrics.numberWidth, alignment: .trailing)
    }
}

// the monospaced pattern field, red when Rust rejected what's in it
private struct SettingsPatternField: View {
    @Binding var text: String
    let placeholder: String
    let invalid: Bool

    var body: some View {
        TextField("Pattern", text: $text, prompt: Text(verbatim: placeholder))
            .labelsHidden()
            .textFieldStyle(.roundedBorder)
            .font(.system(.body, design: .monospaced))
            .foregroundStyle(invalid ? Color.red : Color.primary)
            .overlay(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .strokeBorder(Color.red, lineWidth: invalid ? 1.5 : 0)
                    .allowsHitTesting(false)
            )
    }
}

// "from Slack" / "+ from app": a small menu over the source apps
private struct SettingsSourceChip: View {
    let options: [SettingsOption]
    let selection: String
    let label: String
    var placeholder = false
    let onPick: (String) -> Void

    var body: some View {
        Menu {
            SettingsSourceChoices(options: options, selection: selection, onPick: onPick)
        } label: {
            Text(verbatim: label)
                .foregroundStyle(placeholder ? .secondary : .primary)
        }
        .menuStyle(.button)
        .controlSize(.small)
        .fixedSize()
        .help("Only links from this app")
    }
}

// the source apps with a checkmark on the current one
private struct SettingsSourceChoices: View {
    let options: [SettingsOption]
    let selection: String
    let onPick: (String) -> Void

    var body: some View {
        Picker("Only links from", selection: Binding(get: { selection }, set: { if $0 != selection { onPick($0) } })) {
            ForEach(options, id: \.id) { option in
                Text(verbatim: option.label).tag(option.id)
            }
        }
        .pickerStyle(.inline)
        .labelsHidden()
    }
}

private struct SettingsRuleRow: View {
    @ObservedObject var model: SettingsModel
    let index: Int
    let rule: SettingsRule
    let grip: SettingsReorderGrip

    @State private var text = ""
    @FocusState private var focused: Bool

    private var rules: SettingsRulesState { model.state.rules }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: RuleMetrics.spacing) {
                HStack(spacing: RuleMetrics.spacing) {
                    SettingsDragHandle()
                    SettingsRuleNumber(number: index + 1, accent: rule.matched)
                }
                .contentShape(Rectangle())
                .modifier(grip)
                SettingsPatternField(
                    text: Binding(get: { text }, set: { newText in
                        guard newText != text else { return }
                        text = newText
                        model.editRulePattern(index, newText)
                    }),
                    placeholder: "any link",
                    invalid: rule.patternInvalid
                )
                .focused($focused)
                if rule.patternInvalid {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                        .help("This pattern isn't valid, so it isn't saved")
                }
                SettingsMoreMenu {
                    Menu("Only links from") {
                        SettingsSourceChoices(options: rules.sourceOptions, selection: rule.sourceId) {
                            model.pickRuleSource(index, $0)
                        }
                    }
                    Divider()
                    Button("Duplicate") { model.duplicateRule(index) }
                    Button("Delete", role: .destructive) { model.deleteRule(index) }
                }
                .foregroundStyle(.secondary)
            }
            HStack(spacing: RuleMetrics.spacing) {
                Color.clear.frame(width: RuleMetrics.indent, height: 1)
                if !rule.sourceLabel.isEmpty {
                    SettingsSourceChip(
                        options: rules.sourceOptions, selection: rule.sourceId, label: "from \(rule.sourceLabel)"
                    ) { model.pickRuleSource(index, $0) }
                }
                Image(systemName: "arrow.right")
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("opens in")
                SettingsOptionPicker(
                    options: rules.targetOptions,
                    selection: rule.targetId,
                    label: rule.targetLabel,
                    iconPath: rule.targetIconPath
                ) { model.pickRuleTarget(index, $0) }
                .frame(maxWidth: 240, alignment: .leading)
                .fixedSize()
                if rule.supportsPrivate {
                    SettingsPrivateToggle(isOn: rule.isPrivate) { model.setRulePrivate(index, $0) }
                }
                Spacer(minLength: 0)
            }
        }
        .padding(.vertical, 2)
        .modifier(SettingsHighlight(active: rule.matched))
        .onAppear { text = rule.displayedPattern }
        // Rust's answer never overwrites what's being typed, only a field that isn't focused
        .onChange(of: rule.displayedPattern) { pattern in
            if !focused { text = pattern }
        }
        .onChange(of: focused) { isFocused in
            if !isFocused { text = rule.displayedPattern }
        }
    }
}

// the new rule, saved by Rust as soon as it's complete
private struct SettingsDraftRow: View {
    static let scrollID = "draft"

    @ObservedObject var model: SettingsModel
    let draft: SettingsDraft

    @FocusState private var focused: Bool

    private var rules: SettingsRulesState { model.state.rules }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: RuleMetrics.spacing) {
                Color.clear.frame(width: SettingsDragHandle.width, height: 1)
                SettingsRuleNumber(number: rules.rules.count + 1, accent: true)
                SettingsPatternField(
                    text: Binding(get: { model.draftPattern }, set: { model.setDraftPattern($0) }),
                    placeholder: draft.patternPlaceholder,
                    invalid: draft.patternInvalid
                )
                .focused($focused)
                .onSubmit { model.commitDraftPattern() }
                Button {
                    model.discardDraft()
                } label: {
                    Image(systemName: "xmark")
                        .frame(width: 16, height: 16)
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .help("Discard new rule")
                .accessibilityLabel("Discard new rule")
            }
            HStack(spacing: RuleMetrics.spacing) {
                Color.clear.frame(width: RuleMetrics.indent, height: 1)
                SettingsSourceChip(
                    options: rules.sourceOptions,
                    selection: draft.sourceId,
                    label: draft.sourceLabel.isEmpty ? "+ from app" : "from \(draft.sourceLabel)",
                    placeholder: draft.sourceLabel.isEmpty
                ) { model.pickDraftSource($0) }
                Image(systemName: "arrow.right")
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("opens in")
                SettingsOptionPicker(
                    options: rules.targetOptions,
                    selection: draft.targetId,
                    label: draft.targetLabel.isEmpty ? "Choose…" : draft.targetLabel,
                    iconPath: rules.targetOptions.first { $0.id == draft.targetId }?.iconPath ?? ""
                ) { model.pickDraftTarget($0) }
                .fixedSize()
                Spacer(minLength: 0)
            }
            if !draft.help.isEmpty {
                HStack(alignment: .firstTextBaseline, spacing: 4) {
                    Color.clear.frame(width: RuleMetrics.indent + RuleMetrics.spacing - 4, height: 1)
                    if draft.patternInvalid {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .font(.caption)
                    }
                    Text(verbatim: draft.help)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .font(.callout)
                .foregroundStyle(draft.patternInvalid ? Color.red : Color.secondary)
            }
        }
        .padding(.vertical, 2)
        .modifier(SettingsHighlight(active: true, dashed: true))
        .onAppear { DispatchQueue.main.async { focused = true } }
        .onChange(of: model.draftFocusRequest) { _ in focused = true }
        .onChange(of: focused) { isFocused in
            if !isFocused { model.commitDraftPattern() }
        }
    }
}

// MARK: everything else

private struct SettingsFallbackRow: View {
    @ObservedObject var model: SettingsModel
    let fallback: SettingsFallback

    var body: some View {
        HStack(spacing: RuleMetrics.spacing) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Everything else")
                Text("When no rule matches")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 12)
            Image(systemName: "arrow.right")
                .foregroundStyle(.secondary)
                .accessibilityLabel("opens in")
            SettingsOptionPicker(
                options: model.state.rules.targetOptions,
                selection: fallback.targetId,
                label: fallback.targetLabel,
                iconPath: fallback.targetIconPath
            ) { model.pickFallbackTarget($0) }
            .fixedSize()
            if fallback.supportsPrivate {
                SettingsPrivateToggle(isOn: fallback.isPrivate) { model.setFallbackPrivate($0) }
            }
        }
        .padding(.vertical, 2)
        .modifier(SettingsHighlight(active: fallback.matched))
    }
}
