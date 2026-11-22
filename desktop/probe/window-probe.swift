// Temporary probe (removed after use): how macOS draws a window styled like Stellarator's main
// window (full-size content, transparent title bar, hidden title, a WKWebView filling it), with
// and without an NSToolbar. Prints the corner radius as measured from a screenshot of each
// window, the traffic lights' frames, and which view receives a click in the title-bar band.
import AppKit
import WebKit

final class Probe: NSObject, NSApplicationDelegate {
    var windows: [(String, NSWindow, WKWebView)] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        let variants: [(String, NSWindow.ToolbarStyle?)] = [
            ("plain", nil), ("unified", .unified), ("compact", .unifiedCompact), ("expanded", .expanded),
        ]
        var x: CGFloat = 40
        for (name, style) in variants {
            let w = NSWindow(
                contentRect: NSRect(x: x, y: 160, width: 420, height: 320),
                styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                backing: .buffered, defer: false)
            x += 450
            w.titlebarAppearsTransparent = true
            w.titleVisibility = .hidden
            w.title = name
            if let style {
                w.toolbar = NSToolbar(identifier: "probe-\(name)")
                w.toolbarStyle = style
            }
            let web = WKWebView(frame: w.contentView!.bounds)
            web.autoresizingMask = [.width, .height]
            web.loadHTMLString("<html><body style='margin:0;background:#ff00ff'></body></html>", baseURL: nil)
            w.contentView!.addSubview(web)
            w.makeKeyAndOrderFront(nil)
            windows.append((name, w, web))
        }
        NSApp.activate(ignoringOtherApps: true)
        DispatchQueue.main.asyncAfter(deadline: .now() + 5) { self.report() }
    }

    func report() {
        print("os: \(ProcessInfo.processInfo.operatingSystemVersionString)")
        print("screen scale: \(NSScreen.main?.backingScaleFactor ?? 0)")
        for (name, w, web) in windows {
            guard let frame = w.contentView?.superview else { continue }
            let h = frame.bounds.height
            print("\n== \(name) (\(type(of: frame)))")
            for selName in ["_cornerRadius", "cornerRadius", "_topCornerRadius", "_effectiveCornerRadius"] {
                for obj in [w as NSObject, frame as NSObject] {
                    let sel = NSSelectorFromString(selName)
                    if obj.responds(to: sel) {
                        typealias Getter = @convention(c) (AnyObject, Selector) -> CGFloat
                        let f = unsafeBitCast(obj.method(for: sel), to: Getter.self)
                        print("  \(type(of: obj)).\(selName) = \(f(obj, sel))")
                    }
                }
            }
            for b: NSWindow.ButtonType in [.closeButton, .miniaturizeButton, .zoomButton] {
                if let btn = w.standardWindowButton(b) {
                    let r = btn.convert(btn.bounds, to: nil)
                    print("  button \(b.rawValue): left \(r.minX) top \(h - r.maxY) size \(r.width)x\(r.height) center (\(r.midX), \(h - r.midY))")
                }
            }
            print("  title bar height (contentLayoutRect inset): \(h - w.contentLayoutRect.maxY)")
            for y in [4.0, 14, 26, 40, 51, 60] {
                for xx in [140.0, 300] {
                    let hit = frame.hitTest(NSPoint(x: xx, y: h - y))
                    let inWeb = hit.map { $0 === web || $0.isDescendant(of: web) } ?? false
                    print("  click at x \(Int(xx)) y \(Int(y)): \(hit.map { String(describing: type(of: $0)) } ?? "nil") webview=\(inWeb)")
                }
            }
            let path = "probe-\(name).png"
            let task = Process()
            task.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
            task.arguments = ["-x", "-o", "-l", String(w.windowNumber), path]
            try? task.run()
            task.waitUntilExit()
            print("  screencapture exit \(task.terminationStatus)")
            measure(path)
        }
        NSApp.terminate(nil)
    }

    /// Transparent run at the start of each of the top 40 rows (top-left corner), in pixels.
    func measure(_ path: String) {
        guard let data = FileManager.default.contents(atPath: path),
              let rep = NSBitmapImageRep(data: data) else {
            print("  no image")
            return
        }
        print("  image \(rep.pixelsWide)x\(rep.pixelsHigh) alpha=\(rep.hasAlpha)")
        var runs: [Int] = []
        for y in 0..<min(60, rep.pixelsHigh) {
            var run = 0
            while run < rep.pixelsWide / 2, (rep.colorAt(x: run, y: y)?.alphaComponent ?? 1) < 0.5 { run += 1 }
            runs.append(run)
        }
        print("  top-left transparent runs: \(runs)")
        var bottom: [Int] = []
        for i in 0..<min(60, rep.pixelsHigh) {
            let y = rep.pixelsHigh - 1 - i
            var run = 0
            while run < rep.pixelsWide / 2, (rep.colorAt(x: run, y: y)?.alphaComponent ?? 1) < 0.5 { run += 1 }
            bottom.append(run)
        }
        print("  bottom-left transparent runs: \(bottom)")
    }
}

let app = NSApplication.shared
let probe = Probe()
app.delegate = probe
app.setActivationPolicy(.regular)
app.run()
