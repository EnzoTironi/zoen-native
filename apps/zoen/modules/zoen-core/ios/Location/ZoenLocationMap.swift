import ExpoModulesCore
import MapKit
import UIKit

@MainActor final class ZoenLocationMap: ExpoView, UIGestureRecognizerDelegate {
  typealias Configuration = ZoenLocationMapConfiguration
  let onPointChange = EventDispatcher()
  private let map = MKMapView()
  private let choose = UIButton(type: .system)
  private var lifetime = ZoenLocationLifetime()
  private var configuration: Configuration?
  private var point: ZoenStaticLocation.Point?
  private var tapTicket: ZoenLocationLifetime.Ticket?
  private lazy var tap = UITapGestureRecognizer(target: self, action: #selector(selectPoint(_:)))
  var configurationJson = ""

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = true
    map.showsUserLocation = false
    map.isRotateEnabled = false
    map.isPitchEnabled = false
    map.pointOfInterestFilter = .excludingAll
    map.selectableMapFeatures = []
    tap.delegate = self
    tap.cancelsTouchesInView = false
    map.addGestureRecognizer(tap)
    choose.configuration = .tinted()
    choose.configuration?.image = UIImage(systemName: "mappin.and.ellipse")
    choose.addTarget(self, action: #selector(selectCenter), for: .touchUpInside)
    addSubview(map)
    addSubview(choose)
    choose.translatesAutoresizingMaskIntoConstraints = false
    NSLayoutConstraint.activate([
      choose.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
      choose.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -12),
      choose.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -12),
      choose.heightAnchor.constraint(greaterThanOrEqualToConstant: 44)
    ])
    NotificationCenter.default.addObserver(self, selector: #selector(retireFromBackground),
      name: UIApplication.didEnterBackgroundNotification, object: nil)
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    map.frame = bounds
  }
  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil { retire() }
    else { activateIfPresented() }
  }
  func configure() {
    guard let next = try? Configuration.decode(configurationJson) else { retire(); return }
    if configuration?.binding != next.binding || configuration?.editable != next.editable {
      tap.isEnabled = false
      tapTicket = nil
      choose.cancelTracking(with: nil)
      lifetime.retire()
    }
    configuration = next
    map.accessibilityLabel = next.accessibilityLabel ?? NSLocalizedString("Map", comment: "Static location map")
    choose.setTitle(next.selectPointLabel ?? NSLocalizedString("Select map center", comment: "Choose a static map point without sending"), for: .normal)
    choose.isHidden = !next.editable
    tap.isEnabled = next.editable
    show(next.point)
    activateIfPresented()
    setNeedsLayout()
  }
  private func activateIfPresented() {
    guard window != nil, UIApplication.shared.applicationState == .active, let configuration else { return }
    if lifetime.capture()?.binding != configuration.binding { _ = try? lifetime.activate(configuration.binding) }
    map.isUserInteractionEnabled = true
    choose.isEnabled = configuration.editable
  }
  func retire() {
    lifetime.retire()
    tapTicket = nil
    tap.isEnabled = false
    choose.cancelTracking(with: nil)
    configuration = nil
    point = nil
    map.removeAnnotations(map.annotations)
    map.isUserInteractionEnabled = false
    choose.isEnabled = false
    choose.isHidden = true
  }
  private func show(_ next: ZoenStaticLocation.Point?) {
    guard point != next else { return }
    point = next
    map.removeAnnotations(map.annotations)
    guard let next else { return }
    let coordinate = CLLocationCoordinate2D(latitude: next.latitude, longitude: next.longitude)
    let pin = MKPointAnnotation()
    pin.coordinate = coordinate
    pin.title = next.label ?? NSLocalizedString("Selected location", comment: "Static selected map pin")
    pin.subtitle = String(format: "%.6f, %.6f", next.latitude, next.longitude)
    map.addAnnotation(pin)
    // No custom animation or implicit location fetch; native pan/zoom respects accessibility.
    map.setRegion(MKCoordinateRegion(center: coordinate, span: MKCoordinateSpan(latitudeDelta: 0.02, longitudeDelta: 0.02)), animated: false)
  }
  @objc private func selectPoint(_ gesture: UITapGestureRecognizer) {
    guard gesture.state == .ended else { return }
    guard let ticket = tapTicket, lifetime.current(ticket) else { return }
    tapTicket = nil
    select(map.convert(gesture.location(in: map), toCoordinateFrom: map))
  }
  @objc private func selectCenter() { select(map.centerCoordinate) }
  private func select(_ coordinate: CLLocationCoordinate2D) {
    guard window != nil, UIApplication.shared.applicationState == .active,
          configuration?.editable == true, let ticket = lifetime.capture(), lifetime.current(ticket),
          let next = try? ZoenStaticLocation.Point(longitude: coordinate.longitude, latitude: coordinate.latitude, label: nil).validated() else { return }
    show(next)
    onPointChange(["binding": ticket.binding, "point": ["longitude": next.longitude, "latitude": next.latitude]])
  }
  func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
    var view = touch.view
    while let current = view, current !== map {
      if current is UIControl { return false }
      view = current.superview
    }
    guard configuration?.editable == true, let ticket = lifetime.capture() else { return false }
    tapTicket = ticket
    return true
  }
  @objc private func retireFromBackground() { retire() }
  deinit { NotificationCenter.default.removeObserver(self) }
}

func zoenLocationMapDefinition() -> ViewDefinition<ZoenLocationMap> {
  View(ZoenLocationMap.self) {
    ViewName("ZoenLocationMap")
    Events("onPointChange")
    Prop("configuration") { (view: ZoenLocationMap, value: String) in view.configurationJson = value }
    OnViewDidUpdateProps { view in view.configure() }
  }
}
