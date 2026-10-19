# JNA's JNI entry points and reflection inspect its classes and members by name.
-keep class com.sun.jna.** { *; }

# UniFFI uses JNA direct mapping: method names are Rust's exported C symbols.
-keepclasseswithmembernames,includedescriptorclasses class xyz.tironi.zoen.core.** {
    native <methods>;
}

# JNA reflects public fields/constructors and FieldOrder annotations for layouts,
# and callback interfaces/implementations for native-to-Kotlin calls.
-keepattributes RuntimeVisibleAnnotations
-keep class xyz.tironi.zoen.core.** extends com.sun.jna.Structure { *; }
-keep class xyz.tironi.zoen.core.** implements com.sun.jna.Callback { *; }

# Optional desktop-only JNA window helpers; Android never calls them.
-dontwarn java.awt.Component
-dontwarn java.awt.GraphicsEnvironment
-dontwarn java.awt.HeadlessException
-dontwarn java.awt.Window
