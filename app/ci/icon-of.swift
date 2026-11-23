// Saves the icon macOS shows for an app bundle (NSWorkspace, as the Finder and the Dock get it)
// as a PNG, to check which icon the bundle carries: Assets.car's (CFBundleIconName) or the .icns.
//
//   swift app/ci/icon-of.swift <App.app> <out.png> [size]

import AppKit

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write("usage: icon-of <App.app> <out.png> [size]\n".data(using: .utf8)!)
    exit(2)
}
let size = args.count > 3 ? Double(args[3]) ?? 512 : 512
let icon = NSWorkspace.shared.icon(forFile: args[1])
icon.size = NSSize(width: size, height: size)
guard let tiff = icon.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:])
else {
    FileHandle.standardError.write("could not render the icon\n".data(using: .utf8)!)
    exit(1)
}
try png.write(to: URL(fileURLWithPath: args[2]))
print("icon of \(args[1]): \(bitmap.pixelsWide) x \(bitmap.pixelsHigh) px, \(icon.representations.count) representations")
