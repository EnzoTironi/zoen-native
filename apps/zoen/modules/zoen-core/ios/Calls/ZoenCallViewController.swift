import UIKit
@preconcurrency import WebRTC

@MainActor
final class ZoenCallViewController: UIViewController {
  var end: (() -> Void)?
  var mute: ((Bool) -> Void)?
  var speaker: ((Bool) -> Void)?
  var camera: ((Bool) -> Void)?
  private let name: String
  private let video: Bool
  private let portuguese: Bool
  private var cameraOn = true
  private let status = UILabel()
  private let remote = RTCMTLVideoView()
  private weak var track: RTCVideoTrack?
  private var muteButton: UIButton?
  private var muted = false
  private var speakerOn = false

  init(name: String, video: Bool, locale: String) { self.name = name; self.video = video; portuguese = locale.hasPrefix("pt"); super.init(nibName: nil, bundle: nil); modalPresentationStyle = .fullScreen }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    remote.videoContentMode = .scaleAspectFit
    remote.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(remote)
    NSLayoutConstraint.activate([
      remote.leadingAnchor.constraint(equalTo: view.leadingAnchor), remote.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      remote.topAnchor.constraint(equalTo: view.topAnchor), remote.bottomAnchor.constraint(equalTo: view.bottomAnchor),
    ])
    let title = UILabel()
    title.text = name; title.font = .preferredFont(forTextStyle: .largeTitle)
    title.adjustsFontForContentSizeCategory = true; title.textAlignment = .center; title.numberOfLines = 2
    status.font = .preferredFont(forTextStyle: .body); status.textColor = .secondaryLabel
    status.adjustsFontForContentSizeCategory = true; status.textAlignment = .center
    let labels = UIStackView(arrangedSubviews: [title, status]); labels.axis = .vertical; labels.spacing = 8
    labels.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(labels)
    let mute = button("mic.slash.fill", label: text("Silenciar", "Mute")) { [weak self] button in
      guard let self else { return }; self.muted.toggle(); self.mute?(self.muted)
      button.isSelected = self.muted; button.accessibilityValue = self.muted ? self.text("Ativado", "On") : self.text("Desativado", "Off")
    }
    muteButton = mute
    let speaker = button("speaker.wave.2.fill", label: text("Alto-falante", "Speaker")) { [weak self] button in
      guard let self else { return }; self.speakerOn.toggle(); self.speaker?(self.speakerOn)
      button.isSelected = self.speakerOn; button.accessibilityValue = self.speakerOn ? self.text("Ativado", "On") : self.text("Desativado", "Off")
    }
    let hangup = button("phone.down.fill", label: text("Encerrar chamada", "End call"), destructive: true) { [weak self] _ in self?.end?() }
    var buttons = [mute, speaker]
    if video {
      buttons.append(button("video.slash.fill", label: text("Desativar câmera", "Turn camera off")) { [weak self] button in
        guard let self else { return }; self.cameraOn.toggle(); self.camera?(self.cameraOn)
        button.isSelected = !self.cameraOn
        button.accessibilityLabel = self.cameraOn ? self.text("Desativar câmera", "Turn camera off") : self.text("Ativar câmera", "Turn camera on")
      })
    }
    buttons.append(hangup)
    let controls = UIStackView(arrangedSubviews: buttons); controls.spacing = 16; controls.distribution = .fillEqually
    controls.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(controls)
    NSLayoutConstraint.activate([
      labels.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 40),
      labels.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 24),
      labels.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -24),
      controls.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -32),
      controls.centerXAnchor.constraint(equalTo: view.centerXAnchor),
      controls.leadingAnchor.constraint(greaterThanOrEqualTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 20),
      controls.trailingAnchor.constraint(lessThanOrEqualTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -20),
    ])
    setPhase("connecting")
  }

  private func button(_ symbol: String, label: String, destructive: Bool = false, action: @escaping (UIButton) -> Void) -> UIButton {
    var configuration = UIButton.Configuration.filled()
    configuration.image = UIImage(systemName: symbol)
    configuration.cornerStyle = .capsule
    configuration.baseBackgroundColor = destructive ? .systemRed : .secondarySystemFill
    configuration.baseForegroundColor = destructive ? .white : .label
    let button = UIButton(configuration: configuration)
    button.accessibilityLabel = label; button.accessibilityValue = destructive ? nil : text("Desativado", "Off")
    button.addAction(UIAction { [weak button] _ in guard let button else { return }; UIImpactFeedbackGenerator(style: .light).impactOccurred(); action(button) }, for: .touchUpInside)
    button.widthAnchor.constraint(equalToConstant: 56).isActive = true
    button.heightAnchor.constraint(equalToConstant: 56).isActive = true
    return button
  }

  private func text(_ pt: String, _ en: String) -> String { portuguese ? pt : en }
  func setPhase(_ phase: String) {
    status.text = phase == "connected" ? (video ? text("Chamada de vídeo", "Video call") : text("Chamada de voz", "Voice call")) : phase == "reconnecting" ? text("Reconectando…", "Reconnecting…") : text("Conectando…", "Connecting…")
  }
  func setMuted(_ value: Bool) {
    muted = value; muteButton?.isSelected = value
    muteButton?.accessibilityValue = value ? text("Ativado", "On") : text("Desativado", "Off")
  }
  func showVideo(_ video: RTCVideoTrack?) {
    track?.remove(remote); track = video; video?.add(remote)
  }
  func presentIfNeeded() {
    guard presentingViewController == nil,
          let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first(where: { $0.activationState == .foregroundActive }),
          let root = scene.windows.first(where: \.isKeyWindow)?.rootViewController else { return }
    var top = root
    while let presented = top.presentedViewController { top = presented }
    if top !== self { top.present(self, animated: true) }
  }
}
