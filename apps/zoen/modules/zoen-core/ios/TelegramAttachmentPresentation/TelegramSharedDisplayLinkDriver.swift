// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit
import Darwin

@MainActor
public protocol SharedDisplayLinkDriverLink: AnyObject {
    var isPaused: Bool { get set }
    
    func invalidate()
}

private let isIpad: Bool = {
    var systemInfo = utsname()
    uname(&systemInfo)
    let modelCode = withUnsafePointer(to: &systemInfo.machine) {
        $0.withMemoryRebound(to: CChar.self, capacity: 1) {
            ptr in String.init(validatingUTF8: ptr)
        }
    }
    
    if let modelCode {
        if modelCode.lowercased().hasPrefix("ipad") {
            return true
        }
    }
    
    return false
}()

@MainActor
public final class SharedDisplayLinkDriver {
    public enum FramesPerSecond: Comparable {
        case fps(Int)
        case max
        
        public static func <(lhs: FramesPerSecond, rhs: FramesPerSecond) -> Bool {
            switch lhs {
            case let .fps(lhsFps):
                switch rhs {
                case let .fps(rhsFps):
                    return lhsFps < rhsFps
                case .max:
                    return true
                }
            case .max:
                return false
            }
        }
    }
    
    public typealias Link = SharedDisplayLinkDriverLink
    
    public static let shared = SharedDisplayLinkDriver()
    
    @MainActor
    public final class LinkImpl: Link {
        private let driver: SharedDisplayLinkDriver
        public let framesPerSecond: FramesPerSecond
        let update: (CGFloat) -> Void
        var isValid: Bool = true
        public var isPaused: Bool = false {
            didSet {
                if self.isPaused != oldValue {
                    self.driver.requestUpdate()
                }
            }
        }
        
        init(driver: SharedDisplayLinkDriver, framesPerSecond: FramesPerSecond, update: @escaping (CGFloat) -> Void) {
            self.driver = driver
            self.framesPerSecond = framesPerSecond
            self.update = update
        }
        
        public func invalidate() {
            self.isValid = false
        }
    }
    
    private final class RequestContext {
        weak var link: LinkImpl?
        let framesPerSecond: FramesPerSecond
        
        var lastDuration: Double = 0.0
        
        init(link: LinkImpl, framesPerSecond: FramesPerSecond) {
            self.link = link
            self.framesPerSecond = framesPerSecond
        }
    }
    
    private var displayLink: CADisplayLink?
    private var requests: [RequestContext] = []
    
    private var isInForeground: Bool = false
    private var isProcessingEvent: Bool = false
    private var isUpdateRequested: Bool = false
    
    private init() {
        let _ = NotificationCenter.default.addObserver(forName: UIApplication.willEnterForegroundNotification, object: nil, queue: nil, using: { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.isInForeground = true
                self.update()
            }
        })
        let _ = NotificationCenter.default.addObserver(forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: nil, using: { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.isInForeground = false
                self.update()
            }
        })
        
        if Bundle.main.bundlePath.hasSuffix(".appex") {
            self.isInForeground = true
        } else {
            switch UIApplication.shared.applicationState {
            case .active:
                self.isInForeground = true
            default:
                self.isInForeground = false
            }
        }
        
        self.update()
    }
    
    public func updateForegroundState(_ isActive: Bool) {
        if self.isInForeground != isActive {
            self.isInForeground = isActive
            self.update()
        }
    }
    
    private func requestUpdate() {
        if self.isProcessingEvent {
            self.isUpdateRequested = true
        } else {
            self.update()
        }
    }
    
    private func update() {
        var hasActiveItems = false
        var maxFramesPerSecond: FramesPerSecond = .fps(30)
        for request in self.requests {
            if let link = request.link, link.isValid && !link.isPaused {
                if link.framesPerSecond > maxFramesPerSecond {
                    maxFramesPerSecond = link.framesPerSecond
                }
                hasActiveItems = true
            }
        }
        
        if self.isInForeground && hasActiveItems {
            let displayLink: CADisplayLink
            if let current = self.displayLink {
                displayLink = current
            } else {
                displayLink = CADisplayLink(target: self, selector: #selector(self.displayLinkEvent))
                self.displayLink = displayLink
                displayLink.add(to: .main, forMode: .common)
            }
            if #available(iOS 15.0, *) {
                let maxFps = Float(UIScreen.main.maximumFramesPerSecond)
                if maxFps > 61.0 {
                    var frameRateRange: CAFrameRateRange
                    switch maxFramesPerSecond {
                    case let .fps(fps):
                        if fps > 60 {
                            frameRateRange = CAFrameRateRange(minimum: 30.0, maximum: 120.0, preferred: 120.0)
                        } else {
                            frameRateRange = .default
                        }
                    case .max:
                        frameRateRange = CAFrameRateRange(minimum: 30.0, maximum: 120.0, preferred: 120.0)
                    }
                    
                    if isIpad {
                        frameRateRange = CAFrameRateRange(minimum: 30.0, maximum: 120.0, preferred: 120.0)
                    }
                    
                    if displayLink.preferredFrameRateRange != frameRateRange {
                        displayLink.preferredFrameRateRange = frameRateRange
                        print("SharedDisplayLinkDriver: switch to \(frameRateRange)")
                    }
                }
            }
            displayLink.isPaused = false
        } else {
            if let displayLink = self.displayLink {
                self.displayLink = nil
                displayLink.invalidate()
            }
        }
    }
    
    @objc private func displayLinkEvent(displayLink: CADisplayLink) {
        self.isProcessingEvent = true
        
        let duration = displayLink.targetTimestamp - displayLink.timestamp
        
        var removeIndices: [Int]?
        loop: for i in 0 ..< self.requests.count {
            let request = self.requests[i]
            if let link = request.link, link.isValid {
                if !link.isPaused {
                    var itemDuration = duration
                    
                    switch request.framesPerSecond {
                    case let .fps(value):
                        let secondsPerFrame = 1.0 / CGFloat(value)
                        itemDuration = secondsPerFrame
                        request.lastDuration += duration
                        if request.lastDuration >= secondsPerFrame * 0.95 {
                            //print("item \(link) accepting cycle: \(request.lastDuration - duration) + \(duration) = \(request.lastDuration) >= \(secondsPerFrame)")
                        } else {
                            //print("item \(link) skipping cycle: \(request.lastDuration - duration) + \(duration) < \(secondsPerFrame)")
                            continue loop
                        }
                    case .max:
                        break
                    }
                    
                    request.lastDuration = 0.0
                    link.update(itemDuration)
                }
            } else {
                if removeIndices == nil {
                    removeIndices = [i]
                } else {
                    removeIndices?.append(i)
                }
            }
        }
        if let removeIndices = removeIndices {
            for index in removeIndices.reversed() {
                self.requests.remove(at: index)
            }
            
            self.isUpdateRequested = true
        }
        
        self.isProcessingEvent = false
        if self.isUpdateRequested {
            self.isUpdateRequested = false
            self.update()
        }
    }
    
    public func add(framesPerSecond: FramesPerSecond = .fps(60), _ update: @escaping (CGFloat) -> Void) -> Link {
        let link = LinkImpl(driver: self, framesPerSecond: framesPerSecond, update: update)
        self.requests.append(RequestContext(link: link, framesPerSecond: framesPerSecond))
        
        self.update()
        
        return link
    }
}

