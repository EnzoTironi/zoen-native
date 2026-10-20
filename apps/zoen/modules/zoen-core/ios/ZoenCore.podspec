Pod::Spec.new do |s|
  s.name           = 'ZoenCore'
  s.version        = '1.0.0'
  s.summary        = 'Zoen shared Rust engine'
  s.description    = 'The authenticated Zoen engine, device vault, and app event boundary.'
  s.author         = 'Enzo Tironi'
  s.homepage       = 'https://github.com/EnzoTironi/zoen-native'
  s.platforms      = {
    :ios => '26.0'
  }
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'
  s.dependency 'ReactNativeEnrichedMarkdown'
  s.dependency 'GoogleSignIn', '~> 10.0'
  s.vendored_frameworks = 'generated/ClientFFI.xcframework'

  # Swift/Objective-C compatibility
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
  }

  s.source_files = "**/*.{h,m,mm,swift,hpp,cpp}"
end
