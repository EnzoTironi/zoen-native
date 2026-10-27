// Exercises the exact production attachment owner extracted from the Expo UI patch.
// Child retains its handler just as UIView retains an attached gesture recognizer.
final class Handler {}
final class Child { var handler: Handler? }

@main struct RNHostTouchBindingTests {
  static func main() {
    var attached = 0
    var detached = 0
    var refuse = false
    let create = { (child: Child) -> Handler? in
      attached += 1
      if refuse { return nil }
      let handler = Handler()
      child.handler = handler
      return handler
    }
    let remove = { (handler: Handler, child: Child) in
      precondition(child.handler === handler)
      detached += 1
      child.handler = nil
    }
    let binding = RNHostTouchBinding<Child, Handler>(attach: create, detach: remove)
    let stable = Child()
    binding.update(layoutRoot: false, child: stable)
    precondition(attached == 0 && detached == 0)
    // Child is already mounted. Only the presented-content prop changes.
    binding.update(layoutRoot: true, child: stable)
    precondition(attached == 1 && stable.handler != nil)
    weak var original: Handler?
    original = stable.handler
    binding.update(layoutRoot: true, child: stable)
    precondition(attached == 1 && detached == 0 && stable.handler === original)
    binding.update(layoutRoot: false, child: stable)
    precondition(detached == 1 && stable.handler == nil && original == nil)
    binding.update(layoutRoot: false, child: stable)
    precondition(detached == 1)
    binding.update(layoutRoot: true, child: stable)
    precondition(attached == 2)
    let replacement = Child()
    binding.update(layoutRoot: true, child: replacement)
    precondition(attached == 3 && detached == 2 && stable.handler == nil)
    binding.update(layoutRoot: true, child: nil)
    precondition(detached == 3 && replacement.handler == nil)
    // onDisappear retires immediately; reappear can own the same child afresh.
    binding.update(layoutRoot: true, child: stable)
    binding.retire()
    binding.retire()
    precondition(detached == 4 && stable.handler == nil)
    binding.update(layoutRoot: true, child: stable)
    precondition(attached == 5)
    binding.retire()
    refuse = true
    binding.update(layoutRoot: true, child: stable)
    precondition(attached == 6 && stable.handler == nil)
    refuse = false
    binding.update(layoutRoot: true, child: stable)
    precondition(attached == 7 && stable.handler != nil)
    binding.retire()
    weak var weakChild: Child?
    weak var weakHandler: Handler?
    do {
      let transient = Child()
      binding.update(layoutRoot: true, child: transient)
      weakChild = transient
      weakHandler = transient.handler
    }
    precondition(weakChild == nil && weakHandler == nil)
    binding.update(layoutRoot: true, child: stable)
    precondition(stable.handler != nil)
    binding.retire()
    do {
      let temporary = RNHostTouchBinding<Child, Handler>(attach: create, detach: remove)
      temporary.update(layoutRoot: true, child: stable)
    }
    precondition(stable.handler == nil)
    print("PASS stable child, late root prop, retirement, replacement, weak ownership and attach refusal")
  }
}
