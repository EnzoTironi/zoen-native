import UIKit
@preconcurrency import WebRTC

@MainActor
final class ZoenCallViewController: UIViewController {
  var end: (() -> Void)?
  var mute: ((Bool) -> Void)?
  var speaker: ((Bool) -> Void)?
  var camera: ((Bool) -> Void)?
  var minimize: (() -> Void)?
  var flipCamera: (() -> Void)?
  private let name: String
  private let video: Bool
  private let portuguese: Bool
  private var cameraOn = true
  private let status = UILabel()
  private let remote = RTCMTLVideoView()
  private weak var track: RTCVideoTrack?
  private let localPreview = RTCMTLVideoView()
  private let previewContainer = UIView()
  private weak var localTrack: RTCVideoTrack?
  private var cameraButton: UIButton?
  private var speakerButton: UIButton?
  private var muteButton: UIButton?
  private var muted = false
  private var speakerOn = false

  init(name: String, video: Bool, locale: String) { self.name = name; self.video = video; portuguese = locale.hasPrefix("pt"); super.init(nibName: nil, bundle: nil); modalPresentationStyle = .fullScreen; modalTransitionStyle = UIAccessibility.isReduceMotionEnabled ? .crossDissolve : .coverVertical }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    remote.videoContentMode = .scaleAspectFit
    remote.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(remote)
    var minimizeConfiguration = UIButton.Configuration.plain()
    minimizeConfiguration.image = UIImage(systemName: "chevron.down")
    minimizeConfiguration.baseForegroundColor = .label
    let minimizeButton = UIButton(configuration: minimizeConfiguration)
    minimizeButton.accessibilityLabel = text("Minimizar chamada", "Minimize call")
    minimizeButton.translatesAutoresizingMaskIntoConstraints = false
    minimizeButton.addAction(UIAction { [weak self] _ in
      UIImpactFeedbackGenerator(style: .light).impactOccurred(); self?.minimize?()
    }, for: .touchUpInside)
    view.addSubview(minimizeButton)
    NSLayoutConstraint.activate([
      remote.leadingAnchor.constraint(equalTo: view.leadingAnchor), remote.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      remote.topAnchor.constraint(equalTo: view.topAnchor), remote.bottomAnchor.constraint(equalTo: view.bottomAnchor),
      minimizeButton.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 12),
      minimizeButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
      minimizeButton.widthAnchor.constraint(equalToConstant: 44), minimizeButton.heightAnchor.constraint(equalToConstant: 44),
    ])
    let title = UILabel()
    title.text = name; title.font = .preferredFont(forTextStyle: .largeTitle)
    title.adjustsFontForContentSizeCategory = true; title.textAlignment = .center; title.numberOfLines = 2
    status.font = .preferredFont(forTextStyle: .body); status.textColor = .secondaryLabel
    status.adjustsFontForContentSizeCategory = true; status.textAlignment = .center
    let labels = UIStackView(arrangedSubviews: [title, status]); labels.axis = .vertical; labels.spacing = 8
    labels.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(labels)
    let mute = button("mic.fill", label: text("Silenciar", "Mute")) { [weak self] button in
      guard let self else { return }; self.muted.toggle(); self.mute?(self.muted)
      button.configuration?.image = UIImage(systemName: self.muted ? "mic.slash.fill" : "mic.fill")
      button.isSelected = self.muted; button.accessibilityValue = self.muted ? self.text("Ativado", "On") : self.text("Desativado", "Off")
    }
    muteButton = mute
    let speaker = button("speaker.wave.2.fill", label: text("Alto-falante", "Speaker")) { [weak self] button in
      guard let self else { return }; self.speakerOn.toggle(); self.speaker?(self.speakerOn)
      button.isSelected = self.speakerOn; button.accessibilityValue = self.speakerOn ? self.text("Ativado", "On") : self.text("Desativado", "Off")
    }
    speakerButton = speaker
    let hangup = button("phone.down.fill", label: text("Encerrar chamada", "End call"), destructive: true) { [weak self] _ in self?.end?() }
    var buttons = [mute, speaker]
    if video {
      let camera = button("video.fill", label: text("Desativar câmera", "Turn camera off")) { [weak self] button in
        guard let self else { return }; self.cameraOn.toggle(); self.camera?(self.cameraOn)
        button.isSelected = !self.cameraOn
        button.configuration?.image = UIImage(systemName: self.cameraOn ? "video.fill" : "video.slash.fill")
        button.accessibilityLabel = self.cameraOn ? self.text("Desativar câmera", "Turn camera off") : self.text("Ativar câmera", "Turn camera on")
      }
      cameraButton = camera; buttons.append(camera)
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
    if video { installLocalPreview(above: controls) }
    setPhase("connecting")
  }

  private func installLocalPreview(above controls: UIView) {
    previewContainer.backgroundColor = .secondarySystemBackground
    previewContainer.layer.cornerRadius = 16
    previewContainer.clipsToBounds = true
    previewContainer.isHidden = true
    previewContainer.translatesAutoresizingMaskIntoConstraints = false
    localPreview.translatesAutoresizingMaskIntoConstraints = false
    localPreview.videoContentMode = .scaleAspectFill
    previewContainer.addSubview(localPreview)
    view.addSubview(previewContainer)
    var configuration = UIButton.Configuration.tinted()
    configuration.image = UIImage(systemName: "arrow.triangle.2.circlepath.camera")
    configuration.baseForegroundColor = .white
    configuration.baseBackgroundColor = .black.withAlphaComponent(0.4)
    configuration.cornerStyle = .capsule
    let flip = UIButton(configuration: configuration)
    flip.translatesAutoresizingMaskIntoConstraints = false
    flip.accessibilityLabel = text("Trocar câmera", "Switch camera")
    flip.addAction(UIAction { [weak self] _ in
      UIImpactFeedbackGenerator(style: .light).impactOccurred(); self?.flipCamera?()
    }, for: .touchUpInside)
    previewContainer.addSubview(flip)
    NSLayoutConstraint.activate([
      previewContainer.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -20),
      previewContainer.bottomAnchor.constraint(equalTo: controls.topAnchor, constant: -20),
      previewContainer.widthAnchor.constraint(equalTo: view.safeAreaLayoutGuide.widthAnchor, multiplier: 0.27),
      previewContainer.heightAnchor.constraint(equalTo: view.safeAreaLayoutGuide.heightAnchor, multiplier: 0.27),
      localPreview.leadingAnchor.constraint(equalTo: previewContainer.leadingAnchor),
      localPreview.trailingAnchor.constraint(equalTo: previewContainer.trailingAnchor),
      localPreview.topAnchor.constraint(equalTo: previewContainer.topAnchor),
      localPreview.bottomAnchor.constraint(equalTo: previewContainer.bottomAnchor),
      flip.trailingAnchor.constraint(equalTo: previewContainer.trailingAnchor, constant: -4),
      flip.bottomAnchor.constraint(equalTo: previewContainer.bottomAnchor, constant: -4),
      flip.widthAnchor.constraint(equalToConstant: 44), flip.heightAnchor.constraint(equalToConstant: 44),
    ])
  }

  func showLocalVideo(_ track: RTCVideoTrack?, front: Bool) {
    localTrack?.remove(localPreview); localTrack = track
    previewContainer.isHidden = track == nil
    localPreview.transform = CGAffineTransform(scaleX: front ? -1 : 1, y: 1)
    if isViewLoaded, view.window != nil { track?.add(localPreview) }
  }

  private func button(_ symbol: String, label: String, destructive: Bool = false, action: @escaping (UIButton) -> Void) -> UIButton {
    var configuration = UIButton.Configuration.filled()
    configuration.image = UIImage(systemName: symbol)
    configuration.cornerStyle = .capsule
    configuration.baseBackgroundColor = destructive ? .systemRed : .secondarySystemFill
    configuration.baseForegroundColor = destructive ? .white : .label
    let button = UIButton(configuration: configuration)
    button.configurationUpdateHandler = { button in
      var appearance = button.configuration
      appearance?.baseBackgroundColor = destructive ? .systemRed : button.isSelected ? .label : .secondarySystemFill
      appearance?.baseForegroundColor = destructive ? .white : button.isSelected ? .systemBackground : .label
      button.configuration = appearance
    }
    button.accessibilityLabel = label; button.accessibilityValue = destructive ? nil : text("Desativado", "Off")
    button.addAction(UIAction { [weak button] _ in guard let button else { return }; UIImpactFeedbackGenerator(style: .light).impactOccurred(); action(button) }, for: .touchUpInside)
    button.widthAnchor.constraint(equalToConstant: 56).isActive = true
    button.heightAnchor.constraint(equalToConstant: 56).isActive = true
    return button
  }

  private func text(_ pt: String, _ en: String) -> String { portuguese ? pt : en }
  func setPhase(_ phase: String) {
    status.text = phase == "connected" ? (video ? text("Chamada de vídeo", "Video call") : text("Chamada de voz", "Voice call")) : phase == "reconnecting" ? text("Reconectando…", "Reconnecting…") : phase == "calling" ? text("Chamando…", "Calling…") : text("Conectando…", "Connecting…")
  }
  func setIntent(_ intent: ZoenCallMediaIntent) {
    loadViewIfNeeded()
    setMuted(intent.muted)
    cameraOn = intent.cameraEnabled; speakerOn = intent.speaker
    cameraButton?.isSelected = !cameraOn; speakerButton?.isSelected = speakerOn
    speakerButton?.accessibilityValue = speakerOn ? text("Ativado", "On") : text("Desativado", "Off")
    cameraButton?.configuration?.image = UIImage(systemName: cameraOn ? "video.fill" : "video.slash.fill")
    cameraButton?.accessibilityLabel = cameraOn ? text("Desativar câmera", "Turn camera off") : text("Ativar câmera", "Turn camera on")
  }
  func setMuted(_ value: Bool) {
    muted = value; muteButton?.isSelected = value
    muteButton?.configuration?.image = UIImage(systemName: value ? "mic.slash.fill" : "mic.fill")
    muteButton?.accessibilityLabel = value ? text("Ativar microfone", "Unmute") : text("Silenciar", "Mute")
    muteButton?.accessibilityValue = value ? text("Ativado", "On") : text("Desativado", "Off")
  }
  func showVideo(_ video: RTCVideoTrack?) {
    track?.remove(remote); track = video
    if isViewLoaded, view.window != nil { video?.add(remote) }
  }
  override func viewDidAppear(_ animated: Bool) { super.viewDidAppear(animated); track?.remove(remote); track?.add(remote); localTrack?.remove(localPreview); localTrack?.add(localPreview) }
  override func viewWillDisappear(_ animated: Bool) { track?.remove(remote); localTrack?.remove(localPreview); super.viewWillDisappear(animated) }
  func presentIfNeeded(completion: @escaping () -> Void) -> Bool {
    guard presentingViewController == nil, !isBeingDismissed,
          let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first(where: { $0.activationState == .foregroundActive }),
          let root = scene.windows.first(where: \.isKeyWindow)?.rootViewController else { return false }
    var top = root
    while let presented = top.presentedViewController { top = presented }
    guard top !== self, !top.isBeingDismissed, !top.isBeingPresented else { return false }
    modalTransitionStyle = UIAccessibility.isReduceMotionEnabled ? .crossDissolve : .coverVertical
    top.present(self, animated: true, completion: completion)
    return true
  }
}
