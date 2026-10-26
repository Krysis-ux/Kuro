import AppKit
import SwiftUI

/// Keys the composer handles itself rather than letting the text view insert them.
enum ComposerKey { case submit, up, down, tab, escape }

/// A growing, plain-text `NSTextView`.
///
/// SwiftUI's text fields do not expose the caret on macOS 14, and the `/`
/// palette needs it to find a command typed mid-sentence. Return sends,
/// Shift-Return and Option-Return insert a line break.
struct ComposerTextView: NSViewRepresentable {
    @Binding var text: String
    @Binding var caret: Int
    /// Set to move the caret after a programmatic edit; cleared once applied.
    @Binding var caretRequest: Int?
    @Binding var height: CGFloat
    var autofocus = true
    /// Return `true` when the key was handled.
    var onKey: (ComposerKey) -> Bool

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let textView = KeyTextView()
        textView.isRichText = false
        textView.allowsUndo = true
        textView.font = .systemFont(ofSize: 14)
        textView.textColor = .labelColor
        textView.drawsBackground = false
        textView.textContainerInset = NSSize(width: 0, height: 2)
        textView.textContainer?.lineFragmentPadding = 4
        textView.textContainer?.widthTracksTextView = true
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.string = text
        textView.delegate = context.coordinator
        textView.focusOnAppear = autofocus
        textView.onResize = { [weak coordinator = context.coordinator] in coordinator?.measure() }

        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.borderType = .noBorder
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.documentView = textView
        context.coordinator.textView = textView
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = context.coordinator.textView else { return }
        if textView.string != text {
            textView.string = text
            context.coordinator.measure()
        }
        if let request = caretRequest {
            let location = min(request, (textView.string as NSString).length)
            textView.setSelectedRange(NSRange(location: location, length: 0))
            textView.window?.makeFirstResponder(textView)
            DispatchQueue.main.async { caretRequest = nil }
        }
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ComposerTextView
        weak var textView: KeyTextView?

        init(_ parent: ComposerTextView) { self.parent = parent }

        func textDidChange(_ notification: Notification) {
            guard let textView else { return }
            parent.text = textView.string
            parent.caret = textView.selectedRange().location
            measure()
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            guard let location = textView?.selectedRange().location else { return }
            // Deferred: this also fires while SwiftUI is updating the view.
            DispatchQueue.main.async { [self] in
                if parent.caret != location { parent.caret = location }
            }
        }

        func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            switch selector {
            case #selector(NSResponder.insertNewline(_:)):
                if NSApp.currentEvent?.modifierFlags.contains(.shift) == true { return false }
                return parent.onKey(.submit)
            case #selector(NSResponder.moveUp(_:)): return parent.onKey(.up)
            case #selector(NSResponder.moveDown(_:)): return parent.onKey(.down)
            case #selector(NSResponder.insertTab(_:)): return parent.onKey(.tab)
            case #selector(NSResponder.cancelOperation(_:)): return parent.onKey(.escape)
            default: return false
            }
        }

        /// Report the height the text needs, so the field grows with it.
        func measure() {
            guard let textView, let layout = textView.layoutManager, let container = textView.textContainer else { return }
            layout.ensureLayout(for: container)
            let needed = ceil(layout.usedRect(for: container).height + textView.textContainerInset.height * 2)
            if abs(parent.height - needed) > 0.5 {
                DispatchQueue.main.async { [self] in parent.height = needed }
            }
        }
    }
}

final class KeyTextView: NSTextView {
    var focusOnAppear = false
    var onResize: (() -> Void)?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if focusOnAppear, let window {
            DispatchQueue.main.async { window.makeFirstResponder(self) }
        }
    }

    override func setFrameSize(_ newSize: NSSize) {
        let widthChanged = newSize.width != frame.width
        super.setFrameSize(newSize)
        if widthChanged { onResize?() }
    }
}
