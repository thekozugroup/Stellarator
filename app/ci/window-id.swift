// Prints the window number and bounds of the first on-screen, normal-level window of the process
// named in the first argument (CGWindowListCopyWindowInfo), for `screencapture -l <number>`.
// Exits 1 when there is none.
//
//   swift app/ci/window-id.swift Stellarator

import CoreGraphics
import Foundation

let owner = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "Stellarator"
let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
let windows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows {
    guard (window[kCGWindowOwnerName as String] as? String) == owner,
          (window[kCGWindowLayer as String] as? Int) == 0,
          let number = window[kCGWindowNumber as String] as? Int
    else { continue }
    let bounds = window[kCGWindowBounds as String] as? [String: Any] ?? [:]
    FileHandle.standardError.write("window \(number) bounds \(bounds)\n".data(using: .utf8)!)
    print(number)
    exit(0)
}
FileHandle.standardError.write("no on-screen window of \(owner)\n".data(using: .utf8)!)
exit(1)
