import UIKit

@MainActor extension ZoenAttachmentFileContent {
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
  func updateActionCell(_ cell: RecentActionCell, indexPath: IndexPath) {
    let gallery = kind == "file" && indexPath.row == 0
    cell.update(title: labels[gallery ? "chooseGallery" : "browseFiles"] ?? "", image: ZoenTelegramAttachmentGlyph.image(gallery ? .image : .cloud), appearance: appearance, enabled: enabled && selected.isEmpty, first: indexPath.row == 0, last: kind == "audio" || indexPath.row == 1)
    cell.accessibilityIdentifier = gallery ? "attachment-recent-gallery" : "attachment-file-browse"
  }
  func updateFileCell(_ cell: RecentFileCell, row: [String: Any]) {
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
    // UITableView's simultaneous press highlight is independent from the
    // extracted source and from canonical recentSelected membership.
    cell.setHighlighted(false, animated: false)
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
      }, didDismiss: { [weak self, weak gesture, weak cell] menu in
        gesture?.cancel()
        if cell?.accessibilityIdentifier == "attachment-recent-file-" + id {
          cell?.setHighlighted(false, animated: false)
        }
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
}
