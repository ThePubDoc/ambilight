// Renders the icon sources (macOS only; the generated PNGs are committed):
//   icons/app-icon.png        1024×1024 app icon, fed to `npx tauri icon`
//   src-tauri/icons/tray.png  64×64 black glyph, used as a macOS template image
// Usage: swift tools/make-icons.swift
import AppKit

func png(_ size: Int, _ draw: (CGFloat, CGContext) -> Void) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                               bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw(CGFloat(size), NSGraphicsContext.current!.cgContext)
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let appIcon = png(1024) { s, ctx in
    let inset = s * 0.09
    let bg = NSBezierPath(roundedRect: NSRect(x: inset, y: inset, width: s - 2 * inset, height: s - 2 * inset),
                          xRadius: s * 0.19, yRadius: s * 0.19)
    NSGradient(colors: [NSColor(calibratedWhite: 0.16, alpha: 1), NSColor(calibratedWhite: 0.06, alpha: 1)])!
        .draw(in: bg, angle: -90)

    let screen = NSRect(x: s * 0.24, y: s * 0.36, width: s * 0.52, height: s * 0.34)
    let colors: [NSColor] = [.systemPink, .systemPurple, .systemBlue, .systemTeal, .systemOrange]
    for (i, color) in colors.enumerated() {
        ctx.saveGState()
        bg.addClip()
        let glow = NSBezierPath(roundedRect: screen.insetBy(dx: -s * 0.02, dy: -s * 0.02), xRadius: s * 0.04, yRadius: s * 0.04)
        ctx.setShadow(offset: .zero, blur: s * 0.12, color: color.withAlphaComponent(0.9).cgColor)
        color.withAlphaComponent(0.9).setStroke()
        glow.lineWidth = s * 0.03
        let dash: [CGFloat] = [s * 0.35, s * 1.4]
        glow.setLineDash(dash, count: 2, phase: CGFloat(i) * s * 0.35)
        glow.stroke()
        ctx.restoreGState()
    }

    NSColor(calibratedWhite: 0.04, alpha: 1).setFill()
    NSBezierPath(roundedRect: screen, xRadius: s * 0.03, yRadius: s * 0.03).fill()
    NSColor(calibratedWhite: 0.3, alpha: 1).setFill()
    NSBezierPath(rect: NSRect(x: s * 0.47, y: s * 0.27, width: s * 0.06, height: s * 0.09)).fill()
    NSBezierPath(roundedRect: NSRect(x: s * 0.38, y: s * 0.25, width: s * 0.24, height: s * 0.03),
                 xRadius: s * 0.015, yRadius: s * 0.015).fill()
}

// Monitor outline with short "glow" ticks around it.
let trayIcon = png(64) { s, _ in
    NSColor.black.setStroke()
    NSColor.black.setFill()
    let screen = NSRect(x: s * 0.22, y: s * 0.34, width: s * 0.56, height: s * 0.38)
    let outline = NSBezierPath(roundedRect: screen, xRadius: s * 0.05, yRadius: s * 0.05)
    outline.lineWidth = s * 0.07
    outline.stroke()
    NSBezierPath(rect: NSRect(x: s * 0.46, y: s * 0.22, width: s * 0.08, height: s * 0.12)).fill()
    NSBezierPath(roundedRect: NSRect(x: s * 0.34, y: s * 0.17, width: s * 0.32, height: s * 0.06),
                 xRadius: s * 0.03, yRadius: s * 0.03).fill()

    let ticks = NSBezierPath()
    ticks.lineWidth = s * 0.06
    ticks.lineCapStyle = .round
    for x in [0.34, 0.5, 0.66] {          // above
        ticks.move(to: NSPoint(x: s * x, y: s * 0.80)); ticks.line(to: NSPoint(x: s * x, y: s * 0.90))
    }
    for y in [0.44, 0.62] {               // left and right
        ticks.move(to: NSPoint(x: s * 0.06, y: s * y)); ticks.line(to: NSPoint(x: s * 0.13, y: s * y))
        ticks.move(to: NSPoint(x: s * 0.87, y: s * y)); ticks.line(to: NSPoint(x: s * 0.94, y: s * y))
    }
    ticks.stroke()
}

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
try! FileManager.default.createDirectory(at: root.appendingPathComponent("icons"), withIntermediateDirectories: true)
try! appIcon.write(to: root.appendingPathComponent("icons/app-icon.png"))
try! trayIcon.write(to: root.appendingPathComponent("src-tauri/icons/tray.png"))
print("Wrote icons/app-icon.png and src-tauri/icons/tray.png")
