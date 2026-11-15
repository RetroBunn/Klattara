# The engine calls these from native code by name (src/plat/android/jni).
-keep class com.dgomeze42.klattara.KlattaraNative { native <methods>; }
-keep interface com.dgomeze42.klattara.KlattaraNative$AudioSink { *; }
-keepclassmembers class * implements com.dgomeze42.klattara.KlattaraNative$AudioSink {
    public int onAudio(byte[], int);
}
