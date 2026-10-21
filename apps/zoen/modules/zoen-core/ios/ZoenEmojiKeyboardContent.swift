import UIKit

// Original UIKit content adapter; pinned Telegram Unicode order/metrics are
// attributed in EmojiKeyboard/NOTICE.md. The shared host alone owns UITextInput.
@MainActor
final class ZoenEmojiKeyboardPanelHost: ZoenKeyboardPanelHost {
  override var allowsTextEditingActions: Bool { true }
  override func admitsInsertion(_ text: String) -> Bool { ZoenEmojiKeyboardData.allowed.contains(text) }
  override func makeContent(send: @escaping (String, [String: Any]) -> Void) -> any ZoenKeyboardPanelContent {
    ZoenEmojiKeyboardContent(send: send)
  }
}

@MainActor
final class ZoenEmojiKeyboardContent: NSObject, ZoenKeyboardPanelContent,
  UICollectionViewDataSource, UICollectionViewDelegateFlowLayout {
  enum Failure: Error { case invalidLabels, retired }
  private let root = ZoenEmojiKeyboardRootView()
  var view: UIView { root }
  private let flow = UICollectionViewFlowLayout()
  private lazy var grid = UICollectionView(frame: .zero, collectionViewLayout: flow)
  private let toolbar = UIView()
  private let separator = UIView()
  private let categories = UIScrollView()
  private let keyboard = UIButton(type: .system)
  private let delete = UIButton(type: .system)
  private var categoryButtons: [String: UIButton] = [:]
  private var emit: ((String, [String: Any]) -> Void)?
  private var appearance: ZoenKeyboardPanelAppearance?
  private var labels: [String: String] = [:]
  private var groups = [ZoenEmojiKeyboardData.Section(key: "recent", emoji: [])] + ZoenEmojiKeyboardData.sections
  private var recent: [String] = []
  private var selectedCategory = "people"
  private var variantOverlay: UIView?
  private var ready = false
  private var retired = false
  private var requestedReducedMotion = false
  private var reducedMotion: Bool { requestedReducedMotion || UIAccessibility.isReduceMotionEnabled }
  private var cellSide: CGFloat = 40
  private let requiredLabels = Set(["keyboard", "delete", "recent", "variants"] + ZoenEmojiKeyboardData.sections.map(\.key))
  private let categoryIcons = ["🕘", "😀", "🐻", "🍔", "⚽️", "🚗", "💡", "🔣", "🏳️"]

  init(send: @escaping (String, [String: Any]) -> Void) {
    emit = send
    super.init()
    root.isUserInteractionEnabled = false
    root.onLayout = { [weak self] in self?.layout() }
    root.onEscape = { [weak self] in
      guard let self, self.variantOverlay != nil else { return false }
      self.dismissVariants()
      return true
    }
    grid.backgroundColor = .clear
    grid.dataSource = self
    grid.delegate = self
    grid.alwaysBounceVertical = true
    grid.showsVerticalScrollIndicator = false
    grid.contentInsetAdjustmentBehavior = .never
    grid.accessibilityIdentifier = "composer-emoji-grid"
    grid.register(ZoenEmojiKeyboardCell.self, forCellWithReuseIdentifier: "emoji")
    grid.register(ZoenEmojiKeyboardHeader.self, forSupplementaryViewOfKind: UICollectionView.elementKindSectionHeader, withReuseIdentifier: "header")
    let hold = UILongPressGestureRecognizer(target: self, action: #selector(showVariants(_:)))
    hold.minimumPressDuration = 0.2
    grid.addGestureRecognizer(hold)
    root.addSubview(grid)
    root.addSubview(toolbar)
    toolbar.addSubview(separator)
    toolbar.addSubview(keyboard)
    toolbar.addSubview(delete)
    toolbar.addSubview(categories)
    categories.showsHorizontalScrollIndicator = false
    categories.contentInsetAdjustmentBehavior = .never
    keyboard.accessibilityIdentifier = "composer-emoji-system-keyboard"
    keyboard.setImage(UIImage(systemName: "keyboard"), for: .normal)
    keyboard.addAction(UIAction { [weak self] _ in self?.send("keyboard") }, for: .touchUpInside)
    delete.accessibilityIdentifier = "composer-emoji-delete"
    delete.setImage(UIImage(systemName: "delete.left"), for: .normal)
    delete.addAction(UIAction { [weak self] _ in self?.send("delete") }, for: .touchUpInside)
    for (index, group) in groups.enumerated() {
      let button = UIButton(type: .system)
      button.setTitle(categoryIcons[index], for: .normal)
      button.titleLabel?.font = .systemFont(ofSize: 21)
      button.accessibilityIdentifier = "composer-emoji-category-\(group.key)"
      button.addAction(UIAction { [weak self] _ in self?.navigate(to: group.key) }, for: .touchUpInside)
      categoryButtons[group.key] = button
      categories.addSubview(button)
    }
  }

  func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws {
    guard !retired else { throw Failure.retired }
    guard let labels = model["emojiLabels"] as? [String: String],
          Set(labels.keys) == requiredLabels,
          labels.values.allSatisfy({ !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.count <= 128 && $0.utf8.count <= 512 }) else { throw Failure.invalidLabels }
    self.labels = labels
    self.appearance = appearance
    requestedReducedMotion = model["reducedMotion"] as? Bool ?? false
    ready = true
    root.backgroundColor = appearance.background
    root.isUserInteractionEnabled = true
    keyboard.tintColor = appearance.secondary
    keyboard.accessibilityLabel = labels["keyboard"]
    delete.tintColor = appearance.secondary
    delete.accessibilityLabel = labels["delete"]
    separator.backgroundColor = appearance.separator
    for (key, button) in categoryButtons {
      button.accessibilityLabel = labels[key]
      button.isEnabled = key != "recent" || !recent.isEmpty
      button.alpha = button.isEnabled ? 1 : 0.4
      button.layer.borderColor = appearance.accent.cgColor
    }
    dismissVariants()
    grid.reloadData()
    root.setNeedsLayout()
    updateCategorySelection()
  }

  func retire() {
    guard !retired else { return }
    retired = true
    ready = false
    emit = nil
    dismissVariants()
    recent.removeAll()
    groups[0] = ZoenEmojiKeyboardData.Section(key: "recent", emoji: [])
    root.isUserInteractionEnabled = false
    grid.dataSource = nil
    grid.delegate = nil
    grid.reloadData()
  }

  private func send(_ action: String) {
    guard ready, !retired else { return }
    dismissVariants()
    emit?(action, [:])
  }
  private func choose(_ emoji: String) {
    guard ready, !retired, ZoenEmojiKeyboardData.allowed.contains(emoji) else { return }
    dismissVariants()
    emit?("insert", ["text": emoji])
    guard !retired else { return }
    recent.removeAll { $0 == emoji }
    recent.insert(emoji, at: 0)
    if recent.count > 40 { recent.removeLast(recent.count - 40) }
    // Do not rebuild/reorder cells under a finger after typing. The explicit
    // Recent/category navigation refreshes this ephemeral projection instead.
    categoryButtons["recent"]?.isEnabled = true
    categoryButtons["recent"]?.alpha = 1
  }
  private func navigate(to key: String) {
    guard ready, !retired, let section = groups.firstIndex(where: { $0.key == key }),
          key != "recent" || !recent.isEmpty else { return }
    dismissVariants()
    groups[0] = ZoenEmojiKeyboardData.Section(key: "recent", emoji: recent)
    grid.reloadData()
    grid.layoutIfNeeded()
    guard !groups[section].emoji.isEmpty else { return }
    selectedCategory = key
    updateCategorySelection()
    grid.scrollToItem(at: IndexPath(item: 0, section: section), at: .top, animated: !reducedMotion)
  }
  private func updateCategorySelection() {
    for (key, button) in categoryButtons {
      button.isSelected = key == selectedCategory
      button.accessibilityTraits = key == selectedCategory ? [.button, .selected] : .button
      button.layer.cornerRadius = 10
      button.layer.borderWidth = key == selectedCategory ? 1 : 0
    }
  }

  private func layout() {
    guard let appearance, root.bounds.width > 0, root.bounds.height > 0 else { return }
    let left = max(0, root.safeAreaInsets.left), right = max(0, root.safeAreaInsets.right)
    let width = max(0, root.bounds.width - left - right)
    let bottom = min(max(0, appearance.bottomInset), root.bounds.height)
    let toolbarHeight = min(40, max(0, root.bounds.height - bottom))
    let height = max(0, root.bounds.height - bottom - toolbarHeight)
    let frame = CGRect(x: left, y: 0, width: width, height: height)
    let changed = grid.frame.size != frame.size
    grid.frame = frame
    toolbar.frame = CGRect(x: left, y: height, width: width, height: toolbarHeight + bottom)
    separator.frame = CGRect(x: 0, y: 0, width: width, height: 1 / max(1, root.traitCollection.displayScale))
    keyboard.frame = CGRect(x: 0, y: 0, width: min(44, width / 2), height: toolbarHeight)
    delete.frame = CGRect(x: max(0, width - 44), y: 0, width: min(44, width / 2), height: toolbarHeight)
    categories.frame = CGRect(x: min(44, width / 2), y: 0, width: max(0, width - 88), height: toolbarHeight)
    for (index, group) in groups.enumerated() {
      categoryButtons[group.key]?.frame = CGRect(x: CGFloat(index) * 44, y: 0, width: 44, height: toolbarHeight)
    }
    categories.contentSize = CGSize(width: CGFloat(groups.count) * 44, height: toolbarHeight)
    // Telegram compact ItemLayout:40pt, >=8columns,9pt vertical spacing;
    // <420pt uses7pt edges/9pt minimum horizontal gap, wider uses5pt/2pt.
    let edge: CGFloat = width >= 420 ? 5 : 7
    let spacing: CGFloat = width >= 420 ? 2 : 9
    let available = max(0, width - edge * 2)
    let count = max(8, Int((available + spacing) / (40 + spacing)))
    let proposed = floor(max(0, available - spacing * CGFloat(count - 1)) / CGFloat(count))
    cellSide = min(40, proposed)
    let scale = max(1, root.traitCollection.displayScale)
    let gap = floor(max(0, available - cellSide * CGFloat(count)) / CGFloat(count - 1) * scale) / scale
    let inset = floor(max(0, width - cellSide * CGFloat(count) - gap * CGFloat(count - 1)) / 2 * scale) / scale
    flow.itemSize = CGSize(width: max(1, cellSide), height: max(1, cellSide))
    flow.minimumInteritemSpacing = gap
    flow.minimumLineSpacing = 9
    flow.sectionInset = UIEdgeInsets(top: 0, left: inset, bottom: 18, right: inset)
    if changed {
      flow.invalidateLayout()
      dismissVariants()
      for case let cell as ZoenEmojiKeyboardCell in grid.visibleCells { cell.setEmojiSize(cellSide) }
    }
  }

  func numberOfSections(in collectionView: UICollectionView) -> Int { groups.count }
  func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int {
    guard groups.indices.contains(section) else { return 0 }
    return ready && !retired ? groups[section].emoji.count : 0
  }
  func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell {
    let cell = collectionView.dequeueReusableCell(withReuseIdentifier: "emoji", for: indexPath)
    guard let cell = cell as? ZoenEmojiKeyboardCell, groups.indices.contains(indexPath.section), groups[indexPath.section].emoji.indices.contains(indexPath.item) else { return cell }
    let emoji = groups[indexPath.section].emoji[indexPath.item]
    cell.configure(emoji: emoji, side: cellSide) { [weak self] in self?.choose(emoji) }
    if ZoenEmojiKeyboardData.variants[emoji] != nil, let label = labels["variants"] {
      cell.accessibilityCustomActions = [UIAccessibilityCustomAction(name: label) { [weak self, weak cell] _ in
        guard let self, let cell, cell.accessibilityLabel == emoji else { return false }
        self.presentVariants(base: emoji, anchor: cell.convert(cell.bounds, to: self.root))
        return true
      }]
    } else { cell.accessibilityCustomActions = nil }
    return cell
  }
  func collectionView(_ collectionView: UICollectionView, didSelectItemAt indexPath: IndexPath) {
    guard ready, !retired, groups.indices.contains(indexPath.section), groups[indexPath.section].emoji.indices.contains(indexPath.item) else { return }
    choose(groups[indexPath.section].emoji[indexPath.item])
  }
  func collectionView(_ collectionView: UICollectionView, viewForSupplementaryElementOfKind kind: String, at indexPath: IndexPath) -> UICollectionReusableView {
    let header = collectionView.dequeueReusableSupplementaryView(ofKind: kind, withReuseIdentifier: "header", for: indexPath)
    if let header = header as? ZoenEmojiKeyboardHeader, groups.indices.contains(indexPath.section) {
      header.label.text = labels[groups[indexPath.section].key]
      header.label.textColor = appearance?.secondary
    }
    return header
  }
  func collectionView(_ collectionView: UICollectionView, layout collectionViewLayout: UICollectionViewLayout, referenceSizeForHeaderInSection section: Int) -> CGSize {
    CGSize(width: collectionView.bounds.width, height: ready && !groups[section].emoji.isEmpty ? 24 : 0)
  }
  func collectionView(_ collectionView: UICollectionView, layout collectionViewLayout: UICollectionViewLayout, insetForSectionAt section: Int) -> UIEdgeInsets {
    groups[section].emoji.isEmpty ? .zero : flow.sectionInset
  }
  func scrollViewDidScroll(_ scrollView: UIScrollView) {
    guard scrollView === grid, ready, !retired,
          let index = grid.indexPathsForVisibleItems.min(by: { $0.section == $1.section ? $0.item < $1.item : $0.section < $1.section }), groups.indices.contains(index.section) else { return }
    selectedCategory = groups[index.section].key
    updateCategorySelection()
  }

  @objc private func showVariants(_ gesture: UILongPressGestureRecognizer) {
    guard gesture.state == .began, ready, !retired,
          let index = grid.indexPathForItem(at: gesture.location(in: grid)),
          groups.indices.contains(index.section), groups[index.section].emoji.indices.contains(index.item),
          let cell = grid.cellForItem(at: index) else { return }
    presentVariants(base: groups[index.section].emoji[index.item], anchor: cell.convert(cell.bounds, to: root))
  }
  private func presentVariants(base: String, anchor: CGRect) {
    guard ready, !retired, let variants = ZoenEmojiKeyboardData.variants[base], !variants.isEmpty, let appearance else { return }
    dismissVariants()
    let overlay = UIView(frame: root.bounds)
    overlay.accessibilityViewIsModal = true
    let outside = UIButton(type: .custom)
    outside.frame = overlay.bounds
    outside.isAccessibilityElement = false
    outside.addAction(UIAction { [weak self] _ in self?.dismissVariants() }, for: .touchUpInside)
    overlay.addSubview(outside)
    let options = [base] + variants
    let width = min(max(0, grid.bounds.width - 16), CGFloat(min(6, options.count)) * 44 + 16)
    guard width > 0, grid.bounds.height >= 60 else { return }
    let scroll = UIScrollView(frame: CGRect(x: min(max(grid.frame.minX + 8, anchor.midX - width / 2), grid.frame.maxX - width - 8), y: min(max(0, anchor.minY - 60), grid.frame.maxY - 60), width: width, height: 60))
    scroll.backgroundColor = appearance.background
    scroll.layer.cornerRadius = 12
    scroll.layer.borderWidth = 1 / max(1, root.traitCollection.displayScale)
    scroll.layer.borderColor = appearance.separator.cgColor
    scroll.accessibilityLabel = labels["variants"]
    scroll.showsHorizontalScrollIndicator = options.count > 6
    for (offset, emoji) in options.enumerated() {
      let button = UIButton(type: .custom)
      button.frame = CGRect(x: 8 + CGFloat(offset) * 44, y: 8, width: 44, height: 44)
      button.setTitle(emoji, for: .normal)
      button.titleLabel?.font = .systemFont(ofSize: 32)
      button.accessibilityLabel = emoji
      button.accessibilityIdentifier = "composer-emoji-variant"
      button.addAction(UIAction { [weak self] _ in self?.choose(emoji) }, for: .touchUpInside)
      scroll.addSubview(button)
    }
    scroll.contentSize = CGSize(width: CGFloat(options.count) * 44 + 16, height: 60)
    overlay.addSubview(scroll)
    variantOverlay = overlay
    root.addSubview(overlay)
    UIAccessibility.post(notification: .layoutChanged, argument: scroll)
  }
  private func dismissVariants() {
    variantOverlay?.removeFromSuperview()
    variantOverlay = nil
  }
}

@MainActor
private final class ZoenEmojiKeyboardRootView: UIView {
  var onLayout: (() -> Void)?
  var onEscape: (() -> Bool)?
  override func accessibilityPerformEscape() -> Bool { onEscape?() ?? false }
  override func layoutSubviews() { super.layoutSubviews(); onLayout?() }
  override func safeAreaInsetsDidChange() { super.safeAreaInsetsDidChange(); setNeedsLayout() }
}

@MainActor
private final class ZoenEmojiKeyboardCell: UICollectionViewCell {
  private let label = UILabel()
  private var activate: (() -> Void)?
  override init(frame: CGRect) {
    super.init(frame: frame)
    isAccessibilityElement = true
    accessibilityTraits = .button
    contentView.addSubview(label)
    label.textAlignment = .center
    label.isAccessibilityElement = false
  }
  @available(*, unavailable)
  required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
  override func layoutSubviews() { super.layoutSubviews(); label.frame = contentView.bounds }
  func configure(emoji: String, side: CGFloat, activate: @escaping () -> Void) {
    label.text = emoji
    accessibilityLabel = emoji
    accessibilityIdentifier = "composer-emoji-item"
    self.activate = activate
    setEmojiSize(side)
  }
  func setEmojiSize(_ side: CGFloat) { label.font = .systemFont(ofSize: max(1, floor(32 * min(1, side / 40)))) }
  override func accessibilityActivate() -> Bool { activate?(); return true }
  override func prepareForReuse() { super.prepareForReuse(); activate = nil; label.text = nil; accessibilityLabel = nil; accessibilityCustomActions = nil }
}

@MainActor
private final class ZoenEmojiKeyboardHeader: UICollectionReusableView {
  let label = UILabel()
  override init(frame: CGRect) {
    super.init(frame: frame)
    label.font = .systemFont(ofSize: 13, weight: .semibold)
    label.accessibilityTraits = .header
    label.isAccessibilityElement = true
    addSubview(label)
  }
  @available(*, unavailable)
  required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }
  override func layoutSubviews() { super.layoutSubviews(); label.frame = bounds.insetBy(dx: 16, dy: 0) }
}
