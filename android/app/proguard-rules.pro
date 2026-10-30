# Referenced from app/build.gradle.kts. Preserve entry points resolved by reflection.

# Room resolves <Database>_Impl by name at runtime.
-keep class * extends androidx.room.RoomDatabase { *; }
-keep class **_Impl { *; }
-keepclassmembers class com.lumen.core.database.model.** { *; }

# Entity/DAO signatures are read through generated code that R8 cannot follow back.
-keep class com.lumen.core.database.dao.** { *; }

# QR import screen instantiates the scanner through reflection.
-keep class com.journeyapps.barcodescanner.** { *; }
-keep class com.google.zxing.** { *; }

# SnakeYAML reads Clash subscriptions as Map/List/scalars. Its optional desktop
# JavaBeans introspection path references APIs absent on Android; no bean binding
# is used. Keep the suppression limited to R8's five missing desktop classes.
-dontwarn java.beans.BeanInfo
-dontwarn java.beans.FeatureDescriptor
-dontwarn java.beans.IntrospectionException
-dontwarn java.beans.Introspector
-dontwarn java.beans.PropertyDescriptor
