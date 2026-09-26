import Darwin
import Foundation

/// macOS attributes privacy (TCC) requests to the *responsible* process — normally the app that launched us
/// (Terminal/VS Code in development). Those apps don't declare Photos/Speech usage, so TCC would kill the
/// helper instead of asking. Re-spawning ourselves with "responsibility disclaimed" makes the helper its own
/// responsible process, so macOS uses the usage descriptions embedded in its Info.plist and shows a prompt.
enum Responsibility {
    private typealias DisclaimFn = @convention(c) (UnsafeMutablePointer<posix_spawnattr_t?>, Int32) -> Int32
    private static let marker = "TSM_BRIDGE_DISCLAIMED"

    /// Returns only in the (re-spawned) child. The parent waits and exits with the child's status.
    static func disclaimIfNeeded() {
        guard ProcessInfo.processInfo.environment[marker] == nil,
              let symbol = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_spawnattrs_setdisclaim")
        else { return }
        let disclaim = unsafeBitCast(symbol, to: DisclaimFn.self)

        var attrs: posix_spawnattr_t?
        posix_spawnattr_init(&attrs)
        defer { posix_spawnattr_destroy(&attrs) }
        guard disclaim(&attrs, 1) == 0 else { return }

        let path = CommandLine.arguments[0].hasPrefix("/") ? CommandLine.arguments[0] : Bundle.main.executablePath ?? CommandLine.arguments[0]
        var env = ProcessInfo.processInfo.environment
        env[marker] = "1"
        let argv: [UnsafeMutablePointer<CChar>?] = CommandLine.arguments.map { strdup($0) } + [nil]
        let envp: [UnsafeMutablePointer<CChar>?] = env.map { strdup("\($0.key)=\($0.value)") } + [nil]
        defer { (argv + envp).forEach { free($0) } }

        var pid: pid_t = 0
        // stdin/stdout/stderr are inherited, so the JSON protocol is unaffected.
        guard posix_spawn(&pid, path, nil, &attrs, argv, envp) == 0 else { return }
        var status: Int32 = 0
        while waitpid(pid, &status, 0) == -1 && errno == EINTR {}
        let code = (status & 0x7f) == 0 ? (status >> 8) & 0xff : 128 + (status & 0x7f)
        exit(code)
    }
}
