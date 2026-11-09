//! `drive` — synthetic input + window capture for verifying the UI by hand.
//!
//! AppleScript's `click` is an accessibility action and never reaches an
//! Freya/winit window, so verification needs real CGEvents. Build once:
//!
//!     swiftc -O -o /tmp/drive tools/drive.swift
//!     /tmp/drive winid [name|pid]      # -> "<id> <x> <y> <w> <h>"
//!     /tmp/drive click 900 500 right
//!     /tmp/drive drag 900 500 1100 560 middle
//!     /tmp/drive key n cmd | hold space down | type "hi" | scroll x y dy n cmd
//!
//! Window coordinates are global; add the origin from `winid` to a coordinate
//! read off a screenshot of the window.

import CoreGraphics
import Foundation

// Synthetic input for UI verification: real CGEvents, which Freya/winit sees
// (AppleScript's `click` is an accessibility action and never reaches it).
func post(_ e: CGEvent?) { e?.post(tap: .cghidEventTap); usleep(60_000) }

func mouse(_ type: CGEventType, _ p: CGPoint, _ button: CGMouseButton, _ clicks: Int64 = 1) {
    let e = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: button)
    e?.setIntegerValueField(.mouseEventClickState, value: clicks)
    post(e)
}

func click(_ p: CGPoint, _ name: String, _ clicks: Int) {
    let (down, up, button): (CGEventType, CGEventType, CGMouseButton) = switch name {
    case "right": (.rightMouseDown, .rightMouseUp, .right)
    case "middle": (.otherMouseDown, .otherMouseUp, .center)
    default: (.leftMouseDown, .leftMouseUp, .left)
    }
    post(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left))
    for i in 1...max(1, clicks) {
        mouse(down, p, button, Int64(i))
        mouse(up, p, button, Int64(i))
    }
}

func drag(_ a: CGPoint, _ b: CGPoint, _ name: String) {
    let (down, dragged, up, button): (CGEventType, CGEventType, CGEventType, CGMouseButton) = switch name {
    case "middle": (.otherMouseDown, .otherMouseDragged, .otherMouseUp, .center)
    default: (.leftMouseDown, .leftMouseDragged, .leftMouseUp, .left)
    }
    post(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: a, mouseButton: .left))
    mouse(down, a, button)
    for i in 1...12 {
        let f = Double(i) / 12.0
        mouse(dragged, CGPoint(x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f), button)
    }
    mouse(up, b, button)
}

let keys: [String: CGKeyCode] = [
    "return": 36, "tab": 48, "space": 49, "delete": 51, "escape": 53,
    "left": 123, "right": 124, "down": 125, "up": 126,
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9,
    "b": 11, "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "o": 31, "u": 32,
    "i": 34, "p": 35, "l": 37, "j": 38, "k": 40, "n": 45, "m": 46,
    "1": 18, "2": 19, "3": 20, "4": 21, "0": 29,
]

func flags(_ mods: [String]) -> CGEventFlags {
    var f: CGEventFlags = []
    for m in mods {
        switch m {
        case "cmd": f.insert(.maskCommand)
        case "shift": f.insert(.maskShift)
        case "alt": f.insert(.maskAlternate)
        case "ctrl": f.insert(.maskControl)
        default: break
        }
    }
    return f
}

func key(_ name: String, _ mods: [String]) {
    guard let code = keys[name] else { FileHandle.standardError.write("unknown key \(name)\n".data(using: .utf8)!); return }
    let f = flags(mods)
    let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true)
    down?.flags = f
    post(down)
    let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false)
    up?.flags = f
    post(up)
}

func hold(_ name: String, _ down: Bool) {
    guard let code = keys[name] else { return }
    post(CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down))
}

// ⚠️ Every event states its flags, empty included. An event built with a nil
// source inherits the *combined session state*, which keeps whatever modifier
// the previous posted event carried — so a `key v cmd` left ⌘ hanging over the
// next `type`, and "ZZ" arrived as ⌘Z ⌘Z. Hours went into hunting that as an
// app bug; it was the driver all along.
func type(_ text: String) {
    for ch in text.unicodeScalars {
        var u = UniChar(ch.value)
        for down in [true, false] {
            let e = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down)
            e?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &u)
            e?.flags = []
            post(e)
        }
    }
}

func scroll(_ p: CGPoint, _ dy: Int32, _ mods: [String], _ steps: Int) {
    post(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left))
    for _ in 0..<steps {
        let e = CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1, wheel1: dy, wheel2: 0, wheel3: 0)
        e?.flags = flags(mods)
        post(e)
    }
}


/// ⚠️ `target` as a name picks the FRONTMOST window with it, which is the
/// wrong one the moment two builds of the same app are up — the usual case
/// while verifying a change against the running one. A pid picks exactly one.
func winid(_ target: String) -> Bool {
    let pid = Int(target)
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    for w in list {
        let matches = pid.map { $0 == w[kCGWindowOwnerPID as String] as? Int }
            ?? (w[kCGWindowOwnerName as String] as? String == target)
        guard matches,
              let id = w[kCGWindowNumber as String] as? Int,
              let b = w[kCGWindowBounds as String] as? [String: Any] else { continue }
        let x = b["X"] as? Double ?? 0, y = b["Y"] as? Double ?? 0
        let width = b["Width"] as? Double ?? 0, height = b["Height"] as? Double ?? 0
        if width < 200 { continue } // skip shadows and menu helpers
        print("\(id) \(Int(x)) \(Int(y)) \(Int(width)) \(Int(height))")
        return true
    }
    return false
}

var args = Array(CommandLine.arguments.dropFirst())
func num(_ i: Int) -> Double { Double(args[i]) ?? 0 }

switch args.first ?? "" {
case "click":
    click(CGPoint(x: num(1), y: num(2)), args.count > 3 ? args[3] : "left", args.count > 4 ? Int(args[4])! : 1)
case "drag":
    drag(CGPoint(x: num(1), y: num(2)), CGPoint(x: num(3), y: num(4)), args.count > 5 ? args[5] : "left")
case "key":
    key(args[1], Array(args.dropFirst(2)))
case "hold":
    hold(args[1], args[2] == "down")
case "type":
    type(args[1])
case "scroll":
    scroll(CGPoint(x: num(1), y: num(2)), Int32(args[3])!, Array(args.dropFirst(5)), Int(args[4]) ?? 1)
case "winid":
    exit(winid(args.count > 1 ? args[1] : "synapso") ? 0 : 1)
case "move":
    post(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: CGPoint(x: num(1), y: num(2)), mouseButton: .left))
default:
    print("usage: drive winid|click|drag|key|hold|type|scroll|move …")
}
