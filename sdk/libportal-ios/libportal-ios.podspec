Pod::Spec.new do |s|
  s.name                = "libportal-ios"
  s.version             = "0.1.0"
  s.summary             = "iOS bindings for the Portal SDK"
  s.homepage            = "https://github.com/TwentyTwoHW/portal-software"
  s.license             = "MIT or APACHE-2.0"
  s.authors             = "Alekos Filini"

  s.source_files        = "Sources/LibPortal/**/*.swift"
  s.ios.vendored_frameworks = 'portalFFI.xcframework'

  s.swift_version       = '4.0'
  s.platform            = :ios
  s.ios.deployment_target = '13.0'
  s.source              = { :git => "https://github.com/TwentyTwoHW/portal-software.git", :tag => "sdk-#{s.version}" }

  s.library = 'c++'
 
  s.default_subspec = 'Core'
 
  s.subspec 'Core' do |core|
    core.vendored_frameworks = 'portalFFI.xcframework'
  end
end
