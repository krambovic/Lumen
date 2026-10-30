package com.lumen.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.camera.core.ImageAnalysis
import androidx.camera.mlkit.vision.MlKitAnalyzer
import androidx.camera.view.CameraController
import androidx.camera.view.LifecycleCameraController
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.FlashlightOn
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.common.Barcode
import com.lumen.ui.screens.*
import com.lumen.ui.theme.LumenTheme
import java.util.concurrent.atomic.AtomicBoolean

/** Lumen owns the camera surface, controls and result contract; no stock scanner UI. */
class PortraitCaptureActivity : ComponentActivity() {
    private val delivered = AtomicBoolean(false)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(statusBarStyle = androidx.activity.SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = androidx.activity.SystemBarStyle.dark(android.graphics.Color.TRANSPARENT))
        val language = getSharedPreferences("lumen_prefs", MODE_PRIVATE).getString("language", "en") ?: "en"
        setContent {
            CompositionLocalProvider(LocalStrings provides stringsForLanguage(language)) {
                LumenTheme { Scanner() }
            }
        }
    }

    @Composable private fun Scanner() {
        val s = LocalStrings.current
        val allowed = ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
        val controller = remember { LifecycleCameraController(this).apply {
            setEnabledUseCases(CameraController.IMAGE_ANALYSIS)
            imageAnalysisBackpressureStrategy = ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST
        } }
        val detector = remember { BarcodeScanning.getClient(BarcodeScannerOptions.Builder()
            .setBarcodeFormats(Barcode.FORMAT_QR_CODE).build()) }
        var failure by remember { mutableStateOf(!allowed) }
        var torch by remember { mutableStateOf(false) }
        DisposableEffect(controller, detector, allowed) {
            if (allowed) runCatching {
                val executor = ContextCompat.getMainExecutor(this@PortraitCaptureActivity)
                controller.setImageAnalysisAnalyzer(executor, MlKitAnalyzer(listOf(detector),
                    ImageAnalysis.COORDINATE_SYSTEM_VIEW_REFERENCED, executor) { result ->
                    val value = result.getValue(detector)?.firstNotNullOfOrNull { it.rawValue?.takeIf(String::isNotBlank) }
                    if (value != null && value.length <= 64 * 1024 && delivered.compareAndSet(false, true)) {
                        setResult(RESULT_OK, Intent().putExtra(RESULT_TEXT, value))
                        finish()
                    }
                })
                controller.bindToLifecycle(this@PortraitCaptureActivity)
            }.onFailure { failure = true }
            onDispose {
                controller.clearImageAnalysisAnalyzer()
                controller.unbind()
                detector.close()
            }
        }
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            if (allowed && !failure) AndroidView(factory = { context ->
                PreviewView(context).apply {
                    implementationMode = PreviewView.ImplementationMode.COMPATIBLE
                    scaleType = PreviewView.ScaleType.FILL_CENTER
                    this.controller = controller
                }
            }, modifier = Modifier.fillMaxSize())
            Column(Modifier.fillMaxSize().safeDrawingPadding().padding(20.dp),
                horizontalAlignment = Alignment.CenterHorizontally) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    FilledTonalIconButton(onClick = { finish() }) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, s.back)
                    }
                    Text(s.importQrCode, Modifier.weight(1f).padding(start = 14.dp),
                        style = MaterialTheme.typography.headlineSmall, color = Color.White)
                }
                Spacer(Modifier.weight(1f))
                if (failure) Text(s.qrCameraError, color = Color.White, style = MaterialTheme.typography.bodyLarge)
                else Box(Modifier.sizeIn(maxWidth = 320.dp).fillMaxWidth().aspectRatio(1f)
                    .border(3.dp, MaterialTheme.colorScheme.primary, RoundedCornerShape(48.dp)))
                Spacer(Modifier.height(24.dp))
                Surface(shape = RoundedCornerShape(24.dp), color = Color.Black.copy(alpha = .7f)) {
                    Text(s.qrHint, Modifier.padding(18.dp), color = Color.White, style = MaterialTheme.typography.bodyLarge)
                }
                Spacer(Modifier.weight(1f))
                FilledTonalButton(onClick = { torch = !torch; controller.enableTorch(torch) }, enabled = allowed && !failure) {
                    Icon(Icons.Default.FlashlightOn, null)
                    Text(s.qrTorch, Modifier.padding(start = 10.dp))
                }
            }
        }
    }

    companion object { const val RESULT_TEXT = "lumen.qr.text" }
}
