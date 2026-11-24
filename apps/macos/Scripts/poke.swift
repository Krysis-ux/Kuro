// Send a command to a debug build of the app. See DebugDriver.swift.
import Foundation
DistributedNotificationCenter.default().postNotificationName(.init("kuro.debug"), object: CommandLine.arguments.dropFirst().joined(separator: " "), userInfo: nil, deliverImmediately: true)
