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
  private let intro = ZoenTelegramAttachmentFileIntroView(frame: .zero)
  private let explanation = UILabel()
  private let stateLabel = UILabel()
  private let spinner = UIActivityIndicatorView(style: .medium)
  private let retry = UIButton(type: .system)
  private let more = UIButton(type: .system)
  private let searchSurface = ZoenTelegramAttachmentGlassView()
  private let searchCloseSurface = ZoenTelegramAttachmentGlassView()
  private let searchField = UITextField()
  private let searchIcon = UIImageView()
  private let searchClose = UIButton(type: .custom)
  private var rows: [[String: Any]] = []
  private var selected: [String] = []
  private var labels: [String: String] = [:]
  private var revision = ""
  private var kind = "file"
  private var query = ""
  private var loading = false
  private var hasMore = false
  private var hasError = false
  private var enabled = true
  private var retired = false
  private var visible = false
  private var extractedMenu: ZoenAttachmentExtractedMenu?
  private weak var extractedCell: UITableViewCell?
  private var deferredTableUpdate = false
  private var deferredTableReload = false
  private var wantsSearchFocus = false
  private var emptyImageVisible = false
  private var emptyTextVisible = false
  private var appearance = ZoenKeyboardPanelAppearance(background: .systemBackground, text: .label, secondary: .secondaryLabel, accent: .systemBlue, separator: .separator, bottomInset: 0)
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
  private func completeDeferredTableUpdate() {
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
  private func emit(_ action: String, id: String? = nil, text: String? = nil) {
    guard !retired, visible, enabled else { return }
    var payload: [String: Any] = ["revision": revision]
    if let id { payload["id"] = id }; if let text { payload["text"] = text }
    send(action, payload)
  }
  func numberOfSections(in tableView: UITableView) -> Int { searching ? 1 : 2 }
  func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
    if !searching && section == 0 { return kind == "file" ? 2 : 1 }
    return searching && query.isEmpty ? 0 : rows.count
  }
  func tableView(_ tableView: UITableView, heightForHeaderInSection section: Int) -> CGFloat {
    // ItemListSectionHeaderItem:225–235, 251: title+13 and the actual
    // other-section 28pt inset. The first action block retains its 24pt inset.
    if searching { return .leastNormalMagnitude }
    return section == 1 && !rows.isEmpty ? 28 + ceil(UIFont.systemFont(ofSize: 13).lineHeight) + 13 : 24
  }
  func tableView(_ tableView: UITableView, viewForHeaderInSection section: Int) -> UIView? {
    guard !searching, section == 1, !rows.isEmpty else { return nil }
    let label = UILabel(); label.font = .systemFont(ofSize: 13); label.textColor = appearance.secondary
    label.text = (labels[kind == "audio" ? "chatsMusic" : "recentlySentFiles"] ?? "").uppercased()
    let wrapper = UIView(); wrapper.addSubview(label); label.frame = CGRect(x: 15, y: 28 + 7, width: max(0, tableView.bounds.width - 30), height: ceil(label.font.lineHeight))
    return wrapper
  }
  func tableView(_ tableView: UITableView, heightForRowAt indexPath: IndexPath) -> CGFloat {
    if !searching && indexPath.section == 0 { return ceil(UIFont.systemFont(ofSize: 17).lineHeight) + 30 }
    return RecentFileCell.rowLayout(width: tableView.bounds.width, scale: view.traitCollection.displayScale, selection: !selected.isEmpty).height
  }
  func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
    if !searching && indexPath.section == 0 {
      let cell = tableView.dequeueReusableCell(withIdentifier: "action", for: indexPath) as! RecentActionCell
      updateActionCell(cell, indexPath: indexPath)
      return cell
    }
    guard rows.indices.contains(indexPath.row), let cell = tableView.dequeueReusableCell(withIdentifier: "file", for: indexPath) as? RecentFileCell else { return UITableViewCell() }
    updateFileCell(cell, row: rows[indexPath.row])
    return cell
  }
  private func updateActionCell(_ cell: RecentActionCell, indexPath: IndexPath) {
    let gallery = kind == "file" && indexPath.row == 0
    cell.update(title: labels[gallery ? "chooseGallery" : "browseFiles"] ?? "", image: ZoenTelegramAttachmentGlyph.image(gallery ? .image : .cloud), appearance: appearance, enabled: enabled && selected.isEmpty, first: indexPath.row == 0, last: kind == "audio" || indexPath.row == 1)
    cell.accessibilityIdentifier = gallery ? "attachment-recent-gallery" : "attachment-file-browse"
  }
  private func updateFileCell(_ cell: RecentFileCell, row: [String: Any]) {
    let id = row["id"] as? String ?? ""
    cell.update(row: row, audio: kind == "audio", selection: !selected.isEmpty,
      order: selected.firstIndex(of: id).map { $0 + 1 }, appearance: appearance, enabled: enabled)
    let expected = revision
    cell.preview = kind == "audio" ? { [weak self] in guard let self, self.revision == expected else { return }; self.searchField.resignFirstResponder(); self.emit("recentPreview", id: id) } : nil
    cell.previewLabel = labels["previewFile"]
    cell.contextGesture.isEnabled = enabled && kind == "file" && !searching
    cell.openContext = { [weak self, weak cell] gesture in
      guard let self, let cell, self.revision == expected else { return false }
      return self.openExtractedMenu(cell: cell, id: id, revision: expected, gesture: gesture)
    }
    cell.contextMenuLabel = labels["more"]
  }
  func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
    tableView.deselectRow(at: indexPath, animated: true)
    if !searching && indexPath.section == 0 { if selected.isEmpty { emit(kind == "file" && indexPath.row == 0 ? "recentGallery" : kind == "audio" ? "audio" : "document") }; return }
    guard rows.indices.contains(indexPath.row), let id = rows[indexPath.row]["id"] as? String else { return }
    searchField.resignFirstResponder(); emit("recentOpen", id: id)
  }
  func tableView(_ tableView: UITableView, contextMenuConfigurationForRowAt indexPath: IndexPath, point: CGPoint) -> UIContextMenuConfiguration? {
    if kind == "file", !searching { return nil }
    guard enabled, searching || indexPath.section == 1, rows.indices.contains(indexPath.row),
          let bytes = UInt64(rows[indexPath.row]["bytes"] as? String ?? ""), bytes > 0, bytes <= 64 << 20,
          let id = rows[indexPath.row]["id"] as? String else { return nil }
    let expected = revision
    return UIContextMenuConfiguration(identifier: id as NSString, previewProvider: nil) { [weak self] _ in
      guard let self, !self.retired, self.revision == expected else { return nil }
      var actions: [UIAction] = [
        UIAction(title: self.labels["previewFile"] ?? "") { [weak self] _ in guard let self, self.revision == expected else { return }; self.emit("recentPreview", id: id) },
      ]
      // The pinned FileSearch rows use selection:.none; multi-selection is
      // available only in the normal recent-files controller.
      if !self.searching { actions.append(UIAction(title: self.labels["selectFile"] ?? "", state: self.selected.contains(id) ? .on : .off) { [weak self] _ in guard let self, self.revision == expected else { return }; self.emit("recentSelect", id: id) }) }
      return UIMenu(children: actions)
    }
  }
  private func openExtractedMenu(cell: RecentFileCell, id: String, revision: String,
                                 gesture: ZoenAttachmentContextGesture) -> Bool {
    guard extractedMenu == nil, !retired, visible, enabled, kind == "file", !searching,
          self.revision == revision, let row = rows.first(where: { $0["id"] as? String == id }),
          let bytes = UInt64(row["bytes"] as? String ?? ""), bytes > 0, bytes <= 64 << 20,
          let window = cell.window, scrollView.indexPath(for: cell) != nil else { return false }
    gesture.endPressedAppearance()
    if let selected = scrollView.indexPathForSelectedRow { scrollView.deselectRow(at: selected, animated: false) }
    let source = cell.extractedSource()
    let actions: [ZoenAttachmentContextMenu.Action] = [
      .init(id: "preview", title: labels["previewFile"] ?? "", image: nil, enabled: true, invocation: .afterContentReturns,
        perform: { [weak self] in guard let self, self.revision == revision else { return }; self.emit("recentPreview", id: id) }),
      .init(id: "select", title: labels["selectFile"] ?? "", image: ZoenTelegramAttachmentGlyph.image(.contextSelect), enabled: true,
        perform: { [weak self] in guard let self, self.revision == revision else { return }; self.emit("recentSelect", id: id) }),
    ]
    let menu = ZoenAttachmentExtractedMenu(source: source, actions: actions, dismissLabel: labels["close"] ?? "",
      appearance: appearance, isValid: { [weak self, weak cell] in
        guard let self, let cell else { return false }
        return !self.retired && self.visible && self.enabled && self.kind == "file" && !self.searching &&
          self.revision == revision && self.scrollView.indexPath(for: cell) != nil &&
          cell.accessibilityIdentifier == "attachment-recent-file-" + id
      }, didDismiss: { [weak self, weak gesture] menu in
        gesture?.cancel()
        if self?.extractedMenu === menu {
          self?.extractedMenu = nil; self?.extractedCell = nil
          self?.completeDeferredTableUpdate()
        }
      })
    extractedMenu = menu; extractedCell = cell
    guard menu.present(in: window) else { extractedMenu = nil; extractedCell = nil; return false }
    gesture.externalUpdated = { [weak menu] view, point in menu?.continueSourceGesture(view: view, point: point) }
    gesture.externalEnded = { [weak menu] perform in menu?.endSourceGesture(performAction: perform) }
    return true
  }
  func tableView(_ tableView: UITableView, didEndDisplaying cell: UITableViewCell, forRowAt indexPath: IndexPath) {
    if cell === extractedCell { extractedMenu?.retire(); extractedMenu = nil; extractedCell = nil }
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

/// ItemListPeerActionItem:215–224,267,310,385. The .generic/.glass
/// geometry and original icon are retained; UITableView provides reuse/AX.
@MainActor private final class RecentActionCell: UITableViewCell {
  private let glyph = UIImageView(), title = UILabel(), separator = UIView()
  override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
    super.init(style: style, reuseIdentifier: reuseIdentifier)
    title.font = .systemFont(ofSize: 17)
    for child in [glyph, title, separator] { contentView.addSubview(child) }
    layer.cornerRadius = 26; layer.cornerCurve = .continuous; clipsToBounds = true
  }
  required init?(coder: NSCoder) { nil }
  func update(title: String, image: UIImage?, appearance: ZoenKeyboardPanelAppearance, enabled: Bool, first: Bool, last: Bool) {
    self.title.text = title; self.title.textColor = appearance.accent
    glyph.image = image; glyph.tintColor = appearance.accent
    backgroundColor = appearance.text.withAlphaComponent(appearance.isDark ? 0.06 : 0.035)
    separator.backgroundColor = appearance.separator; separator.isHidden = last
    layer.maskedCorners = []
    if first { layer.maskedCorners.formUnion([.layerMinXMinYCorner, .layerMaxXMinYCorner]) }
    if last { layer.maskedCorners.formUnion([.layerMinXMaxYCorner, .layerMaxXMaxYCorner]) }
    isUserInteractionEnabled = enabled; contentView.alpha = enabled ? 1 : 0.5
    accessibilityLabel = title; accessibilityTraits = enabled ? [.button] : [.button, .notEnabled]
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    let image = glyph.image?.size ?? .zero, pixel = 1 / max(1, traitCollection.displayScale)
    glyph.frame = CGRect(x: floor((59 - image.width) / 2) + 1, y: floor((bounds.height - image.height) / 2 / pixel) * pixel, width: image.width, height: image.height)
    let height = title.font.lineHeight
    title.frame = CGRect(x: 59, y: floor((bounds.height - height) / 2 / pixel) * pixel + 1, width: max(0, bounds.width - 59 - 16), height: height)
    separator.frame = CGRect(x: 59, y: bounds.height - pixel, width: max(0, bounds.width - 59), height: pixel)
  }
}

@MainActor private final class RecentFileCell: UITableViewCell {
  private let extractable = UIView(), body = UIView(), offset = UIView(), extractedBackground = UIView()
  private let icon = UIImageView(), fileExtension = UILabel(), title = UILabel(), subtitle = UILabel(), check = UILabel(), separator = UIView()
  private let previewButton = UIButton(type: .custom)
  var preview: (() -> Void)? { didSet { previewButton.isHidden = preview == nil } }
  var previewLabel: String? { didSet { previewButton.accessibilityLabel = previewLabel } }
  private var selecting = false
  let contextGesture = ZoenAttachmentContextGesture(target: nil, action: nil)
  var openContext: ((ZoenAttachmentContextGesture) -> Bool)?
  var contextMenuLabel: String? {
    didSet {
      guard contextGesture.isEnabled, let label = contextMenuLabel, !label.isEmpty else { accessibilityCustomActions = nil; return }
      accessibilityCustomActions = [UIAccessibilityCustomAction(name: label) { [weak self] _ in
        guard let self, self.contextGesture.isEnabled else { return false }
        return self.openContext?(self.contextGesture) ?? false
      }]
    }
  }
  override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
    super.init(style: style, reuseIdentifier: reuseIdentifier)
    backgroundColor = .clear
    extractable.accessibilityIdentifier = "attachment-file-extraction-source"
    body.accessibilityIdentifier = "attachment-file-extraction-content"
    contentView.addSubview(extractable); extractable.addSubview(body)
    body.addSubview(extractedBackground); body.addSubview(offset)
    extractedBackground.alpha = 0; extractedBackground.layer.cornerRadius = 14
    for child in [icon, fileExtension, title, subtitle] { offset.addSubview(child) }
    contentView.addSubview(check); contentView.addSubview(separator)
    offset.addSubview(previewButton)
    extractable.addGestureRecognizer(contextGesture)
    contextGesture.shouldBegin = { [weak self] _ in self?.openContext != nil }
    contextGesture.activated = { [weak self] gesture, _ in self?.openContext?(gesture) ?? false }
    contextGesture.activationProgress = { [weak self] progress, state in
      guard let self, self.extractable.bounds.width > 0 else { return }
      let minimum = max(0.7, (self.extractable.bounds.width - 15) / self.extractable.bounds.width)
      let scale = 1 * (1 - progress) + minimum * progress
      let transform = CATransform3DMakeScale(scale, scale, 1)
      let previous = self.extractable.layer.sublayerTransform
      self.extractable.layer.sublayerTransform = UIAccessibility.isReduceMotionEnabled ? CATransform3DIdentity : transform
      if case .ended = state, !UIAccessibility.isReduceMotionEnabled {
        self.extractable.layer.animate(from: NSValue(caTransform3D: previous), to: NSValue(caTransform3D: transform),
          keyPath: "sublayerTransform", timingFunction: CAMediaTimingFunctionName.easeOut.rawValue, duration: 0.2)
      }
    }
    previewButton.addAction(UIAction { [weak self] _ in self?.preview?() }, for: .touchUpInside)
    title.font = .systemFont(ofSize: 16, weight: .semibold); title.numberOfLines = 1
    subtitle.font = .systemFont(ofSize: 14); subtitle.numberOfLines = 1; subtitle.lineBreakMode = .byTruncatingMiddle
    check.textAlignment = .center; check.font = .systemFont(ofSize: 15, weight: .medium); check.clipsToBounds = true
    fileExtension.textAlignment = .center; fileExtension.textColor = .white
  }
  required init?(coder: NSCoder) { nil }
  static func rowLayout(width: CGFloat, scale: CGFloat, selection: Bool) -> ZoenTelegramFileRowLayout {
    ZoenTelegramFileRowLayout.measure(width: width, left: 0, right: 0, titleHeight: ceil(UIFont.systemFont(ofSize: 16, weight: .semibold).lineHeight) + 4, subtitleHeight: ceil(UIFont.systemFont(ofSize: 14).lineHeight) + 4, scale: scale, selection: selection)
  }
  func update(row: [String: Any], audio: Bool, selection: Bool, order: Int?, appearance: ZoenKeyboardPanelAppearance, enabled: Bool) {
    selecting = selection
    let name = row["name"] as? String ?? "", ext = (name as NSString).pathExtension.lowercased()
    title.text = name; title.lineBreakMode = audio ? .byTruncatingTail : .byTruncatingMiddle
    icon.image = audio ? ZoenTelegramAttachmentGlyph.image(.play) : ZoenTelegramFileVisuals.documentIcon(ext)
    icon.contentMode = .center; icon.layer.cornerRadius = audio ? 20 : 12; icon.clipsToBounds = true
    fileExtension.text = audio ? "" : ext
    let size: CGFloat = ext.count > 3 ? 14 : 15
    let font = UIFont.systemFont(ofSize: size, weight: .bold)
    fileExtension.font = UIFont(descriptor: font.fontDescriptor.withDesign(.rounded) ?? font.fontDescriptor, size: size)
    fileExtension.adjustsFontSizeToFitWidth = true; fileExtension.minimumScaleFactor = 12 / size
    let bytes = UInt64(row["bytes"] as? String ?? "") ?? 0
    let byteText = ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    if audio { subtitle.text = byteText + " • " + (row["author_title"] as? String ?? "") }
    else {
      let milliseconds = Double(row["at_ms"] as? String ?? "") ?? 0
      let date = DateFormatter.localizedString(from: Date(timeIntervalSince1970: milliseconds / 1000), dateStyle: .medium, timeStyle: .none)
      subtitle.text = byteText + " • " + date
    }
    check.isHidden = !selection; check.text = order.map(String.init) ?? ""
    check.backgroundColor = order == nil ? .clear : appearance.accent
    check.layer.borderWidth = order == nil ? 1 : 0; check.layer.borderColor = appearance.secondary.cgColor
    check.textColor = .white
    applyAppearance(appearance)
    icon.backgroundColor = audio ? appearance.accent : .clear; icon.tintColor = .white
    let available = enabled && bytes > 0 && bytes <= 64 << 20
    isUserInteractionEnabled = available; contentView.alpha = available ? 1 : 0.5
    accessibilityLabel = name + ", " + (subtitle.text ?? "")
    accessibilityTraits = available ? [.button] : [.button, .notEnabled]
    if order != nil { accessibilityTraits.insert(.selected) }
    accessibilityIdentifier = "attachment-recent-file-" + (row["id"] as? String ?? "")
    setNeedsLayout()
  }
  func applyAppearance(_ value: ZoenKeyboardPanelAppearance) { title.textColor = value.text; subtitle.textColor = value.secondary; separator.backgroundColor = value.separator; backgroundColor = value.text.withAlphaComponent(value.isDark ? 0.06 : 0.035); extractedBackground.backgroundColor = value.isDark ? UIColor(red: 44 / 255, green: 44 / 255, blue: 46 / 255, alpha: 1) : .white }
  func extractedSource() -> ZoenAttachmentExtractedContent {
    ZoenAttachmentExtractedContent(anchor: extractable, content: body) { [weak self] extracted, transition in
      guard let self else { return }
      transition.setAlpha(view: self.extractedBackground, alpha: extracted ? 1 : 0)
      transition.setFrame(view: self.extractedBackground, frame: extracted ? self.body.bounds : CGRect(x: -16, y: 0, width: self.body.bounds.width + 16, height: self.body.bounds.height))
      transition.setPosition(view: self.offset, position: CGPoint(x: self.body.bounds.midX + (extracted ? 12 : 0), y: self.body.bounds.midY))
    }
  }
  override func prepareForReuse() {
    super.prepareForReuse(); contextGesture.cancel(); openContext = nil
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    extractable.frame = contentView.bounds
    if body.superview === extractable { body.frame = extractable.bounds; offset.frame = body.bounds; extractedBackground.frame = body.bounds }
    let frames = Self.rowLayout(width: bounds.width, scale: traitCollection.displayScale, selection: selecting)
    icon.frame = frames.icon; title.frame = frames.title; subtitle.frame = frames.subtitle
    previewButton.frame = frames.icon.insetBy(dx: -8, dy: -8)
    fileExtension.frame = frames.icon.insetBy(dx: 1, dy: 0).offsetBy(dx: 0, dy: 5)
    check.frame = frames.selection; check.layer.cornerRadius = 13; separator.frame = frames.separator
  }
}

// Empty-state geometry: AttachmentFileEmptyItem:117–160 and
// ItemListControllerNode:857–865. Empty state receives intrinsic panel insets;
// only search receives the collapsed sheet's additional bottom inset.
struct ZoenAttachmentFileEmptyLayout {
  let image: CGRect
  let text: CGRect
  let showsImage: Bool
  let showsText: Bool
  static func measure(size: CGSize, textSize: CGSize, navigationHeight: CGFloat,
                      actionRows: Int, actionHeight: CGFloat, bottomInset: CGFloat,
                      additionalBottomInset: CGFloat, left: CGFloat, right: CGFloat) -> Self {
    let imageSize = CGSize(width: size.width == 320 ? 112 : 144, height: size.width == 320 ? 112 : 144)
    let topInset: CGFloat = size.width == 320 ? -60 : 120
    let intrinsicBottom = max(0, bottomInset - additionalBottomInset)
    let sourceImageHeight: CGFloat = size.width < size.height ? imageSize.height + 12 : 0
    // Source reserves the optional button geometry even for .intro, where the
    // actual button is absent. The Gallery/Files actions remain separate rows.
    let sourceTotal = sourceImageHeight + 12 + textSize.height + 15 + 52 + 33
    var top = topInset + floor((size.height - topInset - intrinsicBottom - sourceTotal) / 2)
    var imageHeight = sourceImageHeight
    let actionsBottom = navigationHeight + 24 + CGFloat(actionRows) * actionHeight
    let contentTop = actionsBottom + 12, contentBottom = max(0, size.height - bottomInset)
    let freeHeight = max(0, contentBottom - contentTop)
    let showsText = freeHeight >= textSize.height
    let preferredTextBottom = top + imageHeight + 12 + textSize.height
    var textY = top + imageHeight + 12
    // Keep the source target whenever it fits. Compact viewports reuse its
    // no-artwork mode rather than painting decorative content over actions or
    // the keyboard/category panel. The remaining text stays in the free area.
    if top < contentTop || preferredTextBottom > contentBottom {
      if imageHeight + 12 + textSize.height > freeHeight { imageHeight = 0 }
      let height = imageHeight > 0 ? imageHeight + 12 + textSize.height : textSize.height
      top = contentTop + floor((freeHeight - height) / 2)
      textY = imageHeight > 0 ? top + imageHeight + 12 : top
    }
    let imageFrame = CGRect(x: floor((size.width - imageSize.width) / 2), y: top, width: imageSize.width, height: imageSize.height)
    let textX = left + floor((size.width - textSize.width - left - right) / 2)
    let textFrame = CGRect(x: textX, y: textY, width: textSize.width, height: textSize.height)
    let showsImage = showsText && imageHeight > 0
    return Self(image: imageFrame, text: textFrame, showsImage: showsImage, showsText: showsText)
  }
}
