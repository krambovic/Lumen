# Referenced from app/build.gradle.kts. Minification is off today; these rules exist so that
# turning it on does not silently strip the reflectively resolved classes below.

# Room resolves <Database>_Impl by name at runtime.
-keep class * extends androidx.room.RoomDatabase { *; }
-keep class **_Impl { *; }
-keepclassmembers class com.lumen.core.database.model.** { *; }

# Entity/DAO signatures are read through generated code that R8 cannot follow back.
-keep class com.lumen.core.database.dao.** { *; }

# QR import screen instantiates the scanner through reflection.
-keep class com.journeyapps.barcodescanner.** { *; }
-keep class com.google.zxing.** { *; }
