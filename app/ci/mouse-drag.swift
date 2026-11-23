// Drags the mouse with the left button down from one screen point to another (top-left origin,
// as CGWindowListCopyWindowInfo's bounds), the way a person drags a window by its title bar.
// Prints whether this process may post events (Accessibility); without it the system drops them.
//
//   swift app/ci/mouse-drag.swift <x1> <y1> <x2> <y2>

import ApplicationServices
import CoreGraphics
import Foundation

let a = CommandLine.arguments.dropFirst().compactMap { Double($0) }
guard a.count == 4 else {
    FileHandle.standardError.write("usage: mouse-drag x1 y1 x2 y2\n".data(using: .utf8)!)
    exit(2)
}
print("accessibility trusted: \(AXIsProcessTrusted())")
let start = CGPoint(x: a[0], y: a[1]), end = CGPoint(x: a[2], y: a[3])
func post(_ type: CGEventType, _ at: CGPoint) {
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: at, mouseButton: .left)?
        .post(tap: .cghidEventTap)
    usleep(20_000)
}
post(.mouseMoved, start)
usleep(200_000)
post(.leftMouseDown, start)
for step in 1...20 {
    let t = Double(step) / 20
    post(.leftMouseDragged, CGPoint(x: start.x + (end.x - start.x) * t, y: start.y + (end.y - start.y) * t))
}
post(.leftMouseUp, end)
print("dragged from \(start) to \(end)")
