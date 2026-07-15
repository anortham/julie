struct OwnershipBox {
    let value: String

    consuming func take() -> OwnershipBox {
        consume self
    }

    deinit {
        discard self
    }
}

enum SampleError: Error {
    case failed
}

func perform() throws(SampleError) {}

func handle() {
    do throws(SampleError) {
        try perform()
    } catch {
        print(error)
    }
}

class Registry {
    nonisolated(unsafe) static var shared = Registry()

    #if os(iOS)
    static let platformName = "iOS"
    #else
    static let platformName = "other"
    #endif
}

actor Executor {
    nonisolated(nonsending) func schedule() async {}
}

func nestedIndex(_ range: Range<[String].Index>) {}

let formatter = { (value: AnyObject??) -> String? in
    nil
}
