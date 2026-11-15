-keep class music.shard.mobile.NativeBridge { *; }
-keepclasseswithmembernames class * { native <methods>; }
-keep class com.yausername.youtubedl_android.mapper.** { *; }
-keep class com.fasterxml.jackson.** { *; }
-dontwarn org.slf4j.impl.**
# Jackson probes optional Java desktop annotations/XML serializers reflectively.
# These APIs do not exist on Android and are not used by the JSON downloader.
-dontwarn java.beans.ConstructorProperties
-dontwarn java.beans.Transient
-dontwarn org.w3c.dom.bootstrap.DOMImplementationRegistry
