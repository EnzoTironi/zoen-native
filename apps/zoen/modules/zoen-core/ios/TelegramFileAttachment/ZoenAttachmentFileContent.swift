import UIKit

// Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838:
// AttachmentFileController:236–299,313–390,742–761,877–949;
// AttachmentFileSearchItem:956–1187; ListMessageFileItemNode layout and vectors;
// ItemListSelectableControlNode:25–59. GPL-2.0-or-later; GPLv3 selected.
// UIKit hosts the original geometry against Zoen's authorized metadata, not
// Telegram accounts, Postbox, file transport, remote music or a second store.
@MainActor final class ZoenAttachmentFileContent: NSObject, UITableViewDataSource, UITableViewDelegate, UITextFieldDelegate {
  let view = UIView()
  let scrollView = UITableView(frame: .zero, style: .plain)
  var animatedViews: [UIView] { [view, searchSurface, searchCloseSurface] }
  var didBeginEditing: (() -> Void)?
  var hasKeyboardFocus: Bool { searchField.isFirstResponder }
  private(set) var searching = false
  private(set) var searchAvailable = false
  private let send: (String, [String: Any]) -> Void
  let intro = ZoenTelegramAttachmentFileIntroView(frame: .zero)
  private let explanation = UILabel()
  private let stateLabel = UILabel()
  private let spinner = UIActivityIndicatorView(style: .medium)
  private let retry = UIButton(type: .system)
  let more = UIButton(type: .system)
  private let searchSurface = ZoenTelegramAttachmentGlassView()
  private let searchCloseSurface = ZoenTelegramAttachmentGlassView()
  let searchField = UITextField()
  private let searchIcon = UIImageView()
  private let searchClose = UIButton(type: .custom)
  var rows: [[String: Any]] = []
  var selected: [String] = []
  var labels: [String: String] = [:]
  var revision = ""
  var kind = "file"
  var query = ""
  private var loading = false
  private var hasMore = false
  private var hasError = false
  var enabled = true
  var retired = false
  var visible = false
  var extractedMenu: ZoenAttachmentExtractedMenu?
  weak var extractedCell: UITableViewCell?
  private var deferredTableUpdate = false
  private var deferredTableReload = false
  private var wantsSearchFocus = false
  private var emptyImageVisible = false
  private var emptyTextVisible = false
  var appearance = ZoenKeyboardPanelAppearance(background: .systemBackground, text: .label, secondary: .secondaryLabel, accent: .systemBlue, separator: .separator, bottomInset: 0)
  init(send: @escaping (String, [String: Any]) -> Void) {
    self.send = send; super.init()
    scrollView.dataSource = self; scrollView.delegate = self; scrollView.separatorStyle = .none
    scrollView.backgroundColor = .clear; scrollView.contentInsetAdjustmentBehavior = .never
    scrollView.keyboardDismissMode = .onDrag; scrollView.alwaysBounceVertical = true
    scrollView.sectionHeaderTopPadding = 0
    scrollView.accessibilityIdentifier = "attachment-recent-files"
    scrollView.register(RecentFileCell.self, forCellReuseIdentifier: "file")
    scrollView.register(RecentActionCell.self, forCellReuseIdentifier: "action")
    view.addSubview(scrollView)
    for child in [intro, explanation, stateLabel, spinner, retry] { view.addSubview(child) }
    intro.accessibilityIdentifier = "attachment-file-empty-art"
    explanation.accessibilityIdentifier = "attachment-file-empty-text"
    explanation.font = .systemFont(ofSize: 15); explanation.numberOfLines = 0; explanation.textAlignment = .center
    stateLabel.font = .systemFont(ofSize: 15); stateLabel.numberOfLines = 0; stateLabel.textAlignment = .center
    more.titleLabel?.font = .systemFont(ofSize: 17); more.addAction(UIAction { [weak self] _ in self?.emit("recentMore") }, for: .touchUpInside)
    retry.addAction(UIAction { [weak self] _ in self?.emit("recentRetry") }, for: .touchUpInside)
    view.addSubview(searchSurface); view.addSubview(searchCloseSurface)
    searchSurface.contentView.addSubview(searchIcon); searchSurface.contentView.addSubview(searchField)
    searchCloseSurface.contentView.addSubview(searchClose)
    searchIcon.image = ZoenTelegramFileVisuals.searchIcon()
    searchField.font = .systemFont(ofSize: 17); searchField.backgroundColor = .clear
    searchField.clearButtonMode = .whileEditing; searchField.returnKeyType = .search
    searchField.autocorrectionType = .no; searchField.autocapitalizationType = .none; searchField.delegate = self
    searchField.accessibilityIdentifier = "attachment-recent-search"
    searchField.addTarget(self, action: #selector(queryChanged), for: .editingChanged)
    searchClose.setImage(ZoenTelegramFileVisuals.searchCloseIcon(), for: .normal)
    searchClose.addAction(UIAction { [weak self] _ in self?.searchField.resignFirstResponder(); self?.emit("recentSearch", id: "cancel") }, for: .touchUpInside)
  }
  func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws {
    guard !retired, let revision = model["revision"] as? String else { return }
    let kind = model["activeCategory"] as? String == "audio" ? "audio" : "file"
    let rows = model[kind == "audio" ? "audioItems" : "fileItems"] as? [[String: Any]] ?? []
    guard rows.count <= 500, rows.allSatisfy({ row in
      guard let id = row["id"] as? String, !id.isEmpty, id.utf8.count <= 2048, !id.contains("\0"),
            let name = row["name"] as? String, !name.isEmpty, name.utf16.count <= 4096, !name.contains("\0"),
            let bytes = row["bytes"] as? String, UInt64(bytes) != nil,
            let mime = row["mime"] as? String, mime.utf8.count <= 256 else { return false }
      return true
    }), Set(rows.compactMap { $0["id"] as? String }).count == rows.count else { throw Failure.invalid }
    let recent = model["recent"] as? [String: Any] ?? [:]
    let selected = recent["selected"] as? [String] ?? []
    guard selected.count <= 10, Set(selected).count == selected.count else { throw Failure.invalid }
    let nextQuery = recent["query"] as? String ?? ""
    guard nextQuery.utf16.count <= 512, !nextQuery.contains("\0") else { throw Failure.invalid }
    let themeChanged = !self.appearance.background.isEqual(appearance.background) || !self.appearance.text.isEqual(appearance.text) || !self.appearance.accent.isEqual(appearance.accent) || !self.appearance.secondary.isEqual(appearance.secondary) || !self.appearance.separator.isEqual(appearance.separator)
    // Select updates canonical state immediately after beginning the original
    // return. Keep only its real row rendering leased until that return ends;
    // current revision/busy/selection below are never deferred. Disappearance,
    // disabling, category/search/theme changes still retire synchronously.
    let returningSource = extractedCell?.accessibilityIdentifier
    let keepReturning = extractedMenu?.isReturningAfterAction == true && !themeChanged &&
      kind == self.kind && recent["searching"] as? Bool != true && model["busy"] as? Bool != true &&
      rows.contains { "attachment-recent-file-" + ($0["id"] as? String ?? "") == returningSource }
    if !keepReturning {
      extractedMenu?.retire(); extractedMenu = nil
      for case let cell as RecentFileCell in scrollView.visibleCells { cell.contextGesture.cancel() }
    }
    let changed = themeChanged || self.kind != kind || !NSArray(array: rows).isEqual(to: self.rows) || self.selected != selected || self.searching != (recent["searching"] as? Bool == true)
    self.revision = revision; self.rows = rows; self.selected = selected; self.kind = kind; self.appearance = appearance
    labels = model["labels"] as? [String: String] ?? [:]
    loading = recent["loading"] as? Bool == true; hasMore = recent["hasMore"] as? Bool == true
    hasError = recent["error"] as? Bool == true
    let nextSearching = recent["searching"] as? Bool == true
    if nextSearching && !searching { wantsSearchFocus = true }
    searching = nextSearching
    query = nextQuery
    enabled = model["busy"] as? Bool != true
    searchAvailable = selected.isEmpty && (loading || rows.count > 10 || hasMore)
    view.backgroundColor = appearance.background
    searchSurface.update(isDark: appearance.isDark, cornerRadius: 22)
    searchCloseSurface.update(isDark: appearance.isDark, cornerRadius: 22)
    searchField.textColor = appearance.text; searchField.tintColor = appearance.accent
    searchField.keyboardAppearance = appearance.isDark ? .dark : .light
    searchField.attributedPlaceholder = NSAttributedString(string: labels["search"] ?? "", attributes: [.foregroundColor: appearance.secondary])
    searchIcon.tintColor = appearance.secondary; searchClose.tintColor = appearance.text
    searchClose.accessibilityLabel = labels["cancelSearch"]
    if !searchField.isFirstResponder, searchField.text != query { searchField.text = query }
    explanation.text = labels["filesIntro"]?.replacingOccurrences(of: "\n", with: " "); explanation.textColor = appearance.secondary
    stateLabel.text = hasError ? labels["recentError"] : labels["noResults"]; stateLabel.textColor = appearance.secondary
    stateLabel.font = .systemFont(ofSize: hasError ? 15 : 17, weight: hasError ? .regular : .semibold)
    spinner.color = appearance.secondary
    retry.setTitle(labels["retry"], for: .normal); retry.tintColor = appearance.accent
    more.setTitle(labels["loadMore"], for: .normal); more.tintColor = appearance.accent
    more.isEnabled = enabled && !loading; searchField.isEnabled = enabled
    more.accessibilityIdentifier = "attachment-recent-more"
    scrollView.tableFooterView = hasMore && !hasError ? more : nil
    if keepReturning {
      deferredTableUpdate = true; deferredTableReload = deferredTableReload || changed
    } else { reconfigureTable(reload: changed) }
    updateVisibility()
  }
  private func reconfigureTable(reload: Bool) {
    if reload { scrollView.reloadData() }
    else {
      for cell in scrollView.visibleCells {
        guard let path = scrollView.indexPath(for: cell) else { continue }
        if let cell = cell as? RecentFileCell, rows.indices.contains(path.row) { updateFileCell(cell, row: rows[path.row]) }
        else if let cell = cell as? RecentActionCell { updateActionCell(cell, indexPath: path) }
      }
    }
  }
  func completeDeferredTableUpdate() {
    let needsUpdate = deferredTableUpdate, reload = deferredTableReload
    deferredTableUpdate = false; deferredTableReload = false
    guard needsUpdate, !retired else { return }
    reconfigureTable(reload: reload)
  }
  func setVisible(_ value: Bool) {
    guard !retired else { return }
    visible = value
    if !value {
      extractedMenu?.retire(); extractedMenu = nil
      for case let cell as RecentFileCell in scrollView.visibleCells { cell.contextGesture.cancel() }
      searchField.resignFirstResponder(); wantsSearchFocus = false
    }
    intro.setVisible(value && emptyImageVisible && !searching && rows.isEmpty && !loading && !hasError)
  }
  private func updateVisibility() {
    let emptyIntro = !searching && rows.isEmpty && !loading && !hasError
    intro.isHidden = !emptyIntro || !emptyImageVisible; explanation.isHidden = !emptyIntro || !emptyTextVisible
    intro.setVisible(visible && emptyIntro && emptyImageVisible)
    stateLabel.isHidden = loading || (!hasError && (!searching || query.isEmpty || !rows.isEmpty))
    retry.isHidden = !hasError; retry.isEnabled = enabled
    if loading { spinner.startAnimating() } else { spinner.stopAnimating() }
    spinner.isHidden = !loading
    searchSurface.isHidden = !searching; searchCloseSurface.isHidden = !searching
    if !searching { searchField.resignFirstResponder() }
  }
  func updateLayout(navigationHeight: CGFloat, bottomInset: CGFloat, safeArea: UIEdgeInsets, additionalBottomInset: CGFloat = 0) {
    let size = view.bounds.size, width = size.width
    scrollView.frame = CGRect(x: safeArea.left + 16, y: searching ? 56 : navigationHeight, width: max(0, width - safeArea.left - safeArea.right - 32), height: max(0, size.height - (searching ? 56 : navigationHeight)))
    scrollView.contentInset.bottom = bottomInset + 12; scrollView.verticalScrollIndicatorInsets.bottom = bottomInset
    let background = CGRect(x: safeArea.left + 16, y: 6, width: max(0, width - 32 - safeArea.left - safeArea.right - 44 - 8), height: 44)
    searchSurface.frame = background
    searchCloseSurface.frame = CGRect(x: width - 16 - safeArea.right - 44, y: 6, width: 44, height: 44)
    searchClose.frame = searchCloseSurface.bounds
    let iconSize = searchIcon.image?.size ?? .zero, iconScale: CGFloat = 0.8
    searchIcon.frame = CGRect(x: 12, y: floor((44 - iconSize.height * iconScale) / 2), width: iconSize.width * iconScale, height: iconSize.height * iconScale)
    searchField.frame = CGRect(x: 36, y: 0, width: max(0, background.width - 40), height: 44)
    let topInset: CGFloat = width == 320 ? -60 : 120
    let intrinsicBottom = max(0, bottomInset - additionalBottomInset)
    let available = max(0, width - safeArea.left - safeArea.right - 40)
    let textSize = explanation.sizeThatFits(CGSize(width: available, height: max(1, size.height - topInset - intrinsicBottom)))
    let empty = ZoenAttachmentFileEmptyLayout.measure(size: size, textSize: textSize,
      navigationHeight: navigationHeight, actionRows: kind == "file" ? 2 : 1,
      actionHeight: ceil(UIFont.systemFont(ofSize: 17).lineHeight) + 30,
      bottomInset: bottomInset, additionalBottomInset: additionalBottomInset,
      left: safeArea.left, right: safeArea.right)
    intro.frame = empty.image; explanation.frame = empty.text
    emptyImageVisible = empty.showsImage; emptyTextVisible = empty.showsText
    // AttachmentFileSearchItem:862–870 centers the actual empty result in the
    // viewport remaining above the keyboard instead of a fixed top offset.
    let statusSize = stateLabel.sizeThatFits(CGSize(width: max(0, width - safeArea.left - safeArea.right - 32), height: .greatestFiniteMagnitude))
    let statusTop = (searching ? 56 : navigationHeight) + floor((size.height - (searching ? 56 : navigationHeight) - bottomInset - statusSize.height) / 2)
    stateLabel.frame = CGRect(x: floor((width - statusSize.width) / 2), y: statusTop, width: statusSize.width, height: statusSize.height)
    retry.frame = CGRect(x: 24, y: stateLabel.frame.maxY, width: max(0, width - 48), height: 44)
    spinner.center = CGPoint(x: width / 2, y: max(navigationHeight + 88, (size.height - bottomInset) / 2))
    more.frame = CGRect(x: 0, y: 0, width: scrollView.bounds.width, height: 50)
    updateVisibility()
  }
  func focusSearch() {
    guard !retired, visible, searching, enabled, wantsSearchFocus else { return }
    wantsSearchFocus = false
    if searchField.becomeFirstResponder() { didBeginEditing?() }
  }
  func textFieldDidBeginEditing(_ textField: UITextField) { didBeginEditing?() }
  func textFieldShouldReturn(_ textField: UITextField) -> Bool { textField.resignFirstResponder(); return true }
  func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
    guard enabled, let value = textField.text, let swiftRange = Range(range, in: value) else { return false }
    let text = value.replacingCharacters(in: swiftRange, with: string)
    return text.utf16.count <= 512 && !text.contains("\0")
  }
  @objc private func queryChanged() { guard enabled, !retired else { return }; emit("recentQuery", text: searchField.text ?? "") }
  func emit(_ action: String, id: String? = nil, text: String? = nil) {
    guard !retired, visible, enabled else { return }
    var payload: [String: Any] = ["revision": revision]
    if let id { payload["id"] = id }; if let text { payload["text"] = text }
    send(action, payload)
  }

  func retire() {
    guard !retired else { return }
    retired = true; visible = false; enabled = false
    extractedMenu?.retire(); extractedMenu = nil
    for case let cell as RecentFileCell in scrollView.visibleCells { cell.contextGesture.cancel() }
    wantsSearchFocus = false; didBeginEditing = nil
    searchField.resignFirstResponder(); searchField.delegate = nil; intro.retire()
    view.isUserInteractionEnabled = false; view.accessibilityElementsHidden = true
    scrollView.setContentOffset(scrollView.contentOffset, animated: false)
    scrollView.isScrollEnabled = false
    // Retire action authority immediately, preserving the original cells,
    // labels and search text while their owner animates out. Clearing/reloading
    // here flashed the empty illustration before actual presentation removal.
    // These read-only display values die with the removed owner's content;
    // retired guards reject callbacks, late models and attempts to show it again.
  }
  enum Failure: Error { case invalid }
}
