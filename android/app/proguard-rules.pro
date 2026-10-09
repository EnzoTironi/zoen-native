-keep class com.sun.jna.** { *; }
-keep interface com.sun.jna.** { *; }
-keep class xyz.tironi.zoen.core.** { *; }
-keepclassmembers class * extends com.sun.jna.Structure { <fields>; }
# ML Kit's manifest registrar is instantiated through getDeclaredConstructor().
-keep class com.google.mlkit.common.internal.CommonComponentRegistrar {
    public <init>();
}
-dontwarn java.awt.**
-dontwarn javax.swing.**
# PDFBox preserves page streams; Android PdfRenderer handles image decoding.
-dontwarn com.gemalto.jp2.JP2Decoder
-dontwarn com.gemalto.jp2.JP2Encoder
