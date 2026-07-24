package com.lumen.app

import com.journeyapps.barcodescanner.CaptureActivity

/**
 * QR capture activity locked to portrait via AndroidManifest
 * (android:screenOrientation="portrait"). Used by the dashboard QR import so
 * scanning opens a plain camera scanner without rotating the phone to landscape.
 */
class PortraitCaptureActivity : CaptureActivity()
