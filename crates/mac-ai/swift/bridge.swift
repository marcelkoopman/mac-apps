// C-ABI bridge around Apple's Foundation Models framework (on-device model, offline).
// The session gets no tools: the model can only turn text into text.
// Nothing is logged, printed or stored here: text goes in, text comes out, and is gone when the
// caller frees it. No network, no cache, no files.

import Foundation
import FoundationModels

// Status codes, mirrored in src/ffi.rs.
private let statusOk: Int32 = 0
private let statusUnavailable: Int32 = 1
private let statusCancelled: Int32 = 2
private let statusGuardrail: Int32 = 3
private let statusTooLong: Int32 = 4
private let statusLanguage: Int32 = 5
private let statusFailed: Int32 = 6

/// One run that can be cancelled from another thread.
private final class Job: @unchecked Sendable {
    private let lock = NSLock()
    private var task: Task<Void, Never>?
    private var cancelled = false

    func attach(_ task: Task<Void, Never>) {
        lock.lock()
        self.task = task
        let already = cancelled
        lock.unlock()
        if already { task.cancel() }
    }

    func cancel() {
        lock.lock()
        cancelled = true
        let task = self.task
        lock.unlock()
        task?.cancel()
    }
}

private final class Outcome: @unchecked Sendable {
    var status: Int32 = statusFailed
    var text: String = ""
}

/// 0 = available, 1 = device not eligible, 2 = Apple Intelligence not enabled, 3 = model not
/// ready, 4 = unavailable for another reason, 5 = macOS before 26.
@_cdecl("mac_ai_availability")
public func macAiAvailability() -> Int32 {
    guard #available(macOS 26.0, *) else { return 5 }
    switch SystemLanguageModel.default.availability {
    case .available:
        return 0
    case .unavailable(let reason):
        switch reason {
        case .deviceNotEligible: return 1
        case .appleIntelligenceNotEnabled: return 2
        case .modelNotReady: return 3
        @unknown default: return 4
        }
    @unknown default:
        return 4
    }
}

@_cdecl("mac_ai_job_new")
public func macAiJobNew() -> UnsafeMutableRawPointer {
    Unmanaged.passRetained(Job()).toOpaque()
}

@_cdecl("mac_ai_job_cancel")
public func macAiJobCancel(_ handle: UnsafeMutableRawPointer) {
    Unmanaged<Job>.fromOpaque(handle).takeUnretainedValue().cancel()
}

@_cdecl("mac_ai_job_free")
public func macAiJobFree(_ handle: UnsafeMutableRawPointer) {
    Unmanaged<Job>.fromOpaque(handle).release()
}

/// Run the model with `instruction` (its instructions) on the UTF-8 prompt `input`, both built in
/// Rust and passed whole by pointer and byte length (nothing is cut, nothing needs a NUL), and
/// block until it is done or cancelled.
/// On success `*out` is a malloc'ed, NUL-terminated string to free with `mac_ai_string_free`;
/// otherwise it is left null. Call it from a background thread, never the main thread.
@_cdecl("mac_ai_job_run")
public func macAiJobRun(
    _ handle: UnsafeMutableRawPointer,
    _ instruction: UnsafePointer<UInt8>,
    _ instructionLen: Int,
    _ input: UnsafePointer<UInt8>,
    _ len: Int,
    _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
    out.pointee = nil
    guard #available(macOS 26.0, *), macAiAvailability() == 0,
          let system = String(bytes: UnsafeBufferPointer(start: instruction, count: instructionLen), encoding: .utf8),
          let text = String(bytes: UnsafeBufferPointer(start: input, count: len), encoding: .utf8)
    else { return statusUnavailable }
    let job = Unmanaged<Job>.fromOpaque(handle).takeUnretainedValue()
    let outcome = Outcome()
    let done = DispatchSemaphore(value: 0)
    let task = Task {
        defer { done.signal() }
        do {
            let session = LanguageModelSession(instructions: system)
            let response = try await session.respond(to: text, options: GenerationOptions(temperature: 0.3))
            try Task.checkCancellation()
            outcome.text = response.content
            outcome.status = statusOk
        } catch is CancellationError {
            outcome.status = statusCancelled
        } catch let error as LanguageModelSession.GenerationError {
            switch error {
            case .guardrailViolation: outcome.status = statusGuardrail
            case .exceededContextWindowSize: outcome.status = statusTooLong
            case .unsupportedLanguageOrLocale: outcome.status = statusLanguage
            default: outcome.status = Task.isCancelled ? statusCancelled : statusFailed
            }
        } catch {
            outcome.status = Task.isCancelled ? statusCancelled : statusFailed
        }
    }
    job.attach(task)
    done.wait()
    if outcome.status == statusOk {
        out.pointee = strdup(outcome.text)
        if out.pointee == nil { return statusFailed }
    }
    return outcome.status
}

@_cdecl("mac_ai_string_free")
public func macAiStringFree(_ text: UnsafeMutablePointer<CChar>?) {
    free(text)
}
