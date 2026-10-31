from __future__ import annotations

from copy import deepcopy
from pathlib import Path
from types import SimpleNamespace

from xray_fluent import constants
from xray_fluent.models import AppSettings, AppState, Node
from xray_fluent.qml_app.bridge.app_bridge import AppBridge

QML_DIR = Path(__file__).resolve().parents[1] / "xray_fluent" / "qml_app" / "qml"


class _Controller:
    def __init__(self, settings: AppSettings | None = None, *, profile_loaded: bool = True) -> None:
        self.state = AppState(settings=settings or AppSettings())
        self.profile_loaded = profile_loaded
        self.updates: list[AppSettings] = []

    def update_settings(self, settings: AppSettings) -> None:
        self.updates.append(deepcopy(settings))
        self.state.settings = settings


def _bridge(controller: _Controller) -> SimpleNamespace:
    return SimpleNamespace(controller=controller)


def test_onboarding_revision_defaults_and_round_trips() -> None:
    assert AppSettings().onboarding_revision == 0
    assert AppSettings.from_dict({}).onboarding_revision == 0
    assert AppSettings.from_dict({"onboarding_revision": "bad"}).onboarding_revision == 0
    assert AppSettings.from_dict({"onboarding_revision": -3}).onboarding_revision == 0

    settings = AppSettings(onboarding_revision=constants.ONBOARDING_REVISION)
    restored = AppSettings.from_dict(settings.to_dict())
    assert restored.onboarding_revision == constants.ONBOARDING_REVISION


def test_onboarding_is_required_for_fresh_and_upgraded_profiles() -> None:
    fresh = _Controller()
    assert AppBridge.onboardingRequired.fget(_bridge(fresh)) is True
    assert AppBridge.onboardingReturningUser.fget(_bridge(fresh)) is False

    upgraded = _Controller(AppSettings.from_dict({"theme": "dark"}))
    upgraded.state.nodes = [Node(id="n1", name="Server", link="vless://x@example.com:443")]
    assert AppBridge.onboardingRequired.fget(_bridge(upgraded)) is True
    assert AppBridge.onboardingReturningUser.fget(_bridge(upgraded)) is True
    assert AppBridge.onboardingServerCount.fget(_bridge(upgraded)) == 1


def test_onboarding_is_hidden_until_profile_is_unlocked() -> None:
    locked = _Controller(profile_loaded=False)
    assert AppBridge.onboardingRequired.fget(_bridge(locked)) is False


def test_complete_onboarding_persists_current_revision_once() -> None:
    controller = _Controller()
    bridge = _bridge(controller)

    AppBridge.completeOnboarding(bridge)
    assert controller.state.settings.onboarding_revision == constants.ONBOARDING_REVISION
    assert AppBridge.onboardingRequired.fget(bridge) is False

    AppBridge.completeOnboarding(bridge)
    assert len(controller.updates) == 1


def test_profiles_closed_on_the_first_2_0_0_build_get_the_setup_again() -> None:
    # The first 2.0.0 build could keep the welcome card invisible, so a
    # revision written by it must not count as a completed setup.
    assert constants.ONBOARDING_REVISION >= 2
    stale = _Controller(AppSettings(onboarding_revision=1, onboarding_version="2.0.0"))
    bridge = _bridge(stale)
    assert AppBridge.onboardingSetupRequired.fget(bridge) is True
    assert AppBridge.onboardingRequired.fget(bridge) is True
    assert AppBridge.onboardingReturningUser.fget(bridge) is True

    AppBridge.completeOnboarding(bridge)
    assert stale.state.settings.onboarding_revision == constants.ONBOARDING_REVISION
    assert AppBridge.onboardingRequired.fget(bridge) is False


def test_show_onboarding_again_resets_revision() -> None:
    controller = _Controller(AppSettings(onboarding_revision=constants.ONBOARDING_REVISION))
    bridge = _bridge(controller)
    assert AppBridge.onboardingRequired.fget(bridge) is False

    AppBridge.showOnboardingAgain(bridge)
    assert AppBridge.onboardingRequired.fget(bridge) is True


def test_main_window_hosts_onboarding_over_blurred_chrome() -> None:
    main = (QML_DIR / "Main.qml").read_text(encoding="utf-8")
    overlay = (QML_DIR / "OnboardingOverlay.qml").read_text(encoding="utf-8")

    assert "OnboardingOverlay {" in main
    assert "blurSource: appChrome" in main
    assert "win.currentIndex = 0" in main
    assert "App.onboardingRequired && !App.locked" in overlay
    assert "App.completeOnboarding()" in overlay
    assert "App.generateWarpConfigs()" in overlay
    assert "App.openUrl(App.recommendedVpnUrl)" not in overlay
    assert "App.applyRoutingPresetOption(" in overlay
    assert "App.setRegionalPreset(" in overlay
    assert "Theme.animations" in overlay


def test_fresh_install_gets_setup_without_whats_new() -> None:
    bridge = _bridge(_Controller())
    assert AppBridge.onboardingSetupRequired.fget(bridge) is True
    assert AppBridge.onboardingShowWhatsNew.fget(bridge) is False
    assert AppBridge.onboardingPreviousVersion.fget(bridge) == ""


def test_profile_from_older_release_gets_setup_and_whats_new() -> None:
    legacy = AppSettings.from_dict({"theme": "dark"})  # saved by 1.9.x: no onboarding keys
    assert legacy.onboarding_version == "0"
    bridge = _bridge(_Controller(legacy))
    assert AppBridge.onboardingRequired.fget(bridge) is True
    assert AppBridge.onboardingSetupRequired.fget(bridge) is True
    assert AppBridge.onboardingShowWhatsNew.fget(bridge) is True
    assert AppBridge.onboardingReturningUser.fget(bridge) is True
    groups = AppBridge.onboardingWhatsNew.fget(bridge)
    assert groups[0]["version"] == constants.APP_VERSION
    assert groups[0]["items"]


def test_later_update_shows_only_whats_new() -> None:
    settings = AppSettings(onboarding_revision=constants.ONBOARDING_REVISION, onboarding_version="1.0.0")
    bridge = _bridge(_Controller(settings))
    assert AppBridge.onboardingSetupRequired.fget(bridge) is False
    assert AppBridge.onboardingShowWhatsNew.fget(bridge) is True
    assert AppBridge.onboardingRequired.fget(bridge) is True
    assert AppBridge.onboardingPreviousVersion.fget(bridge) == "1.0.0"


def test_complete_onboarding_records_current_version() -> None:
    controller = _Controller(AppSettings.from_dict({}))
    bridge = _bridge(controller)
    AppBridge.completeOnboarding(bridge)
    assert controller.state.settings.onboarding_version == constants.APP_VERSION
    assert AppBridge.onboardingShowWhatsNew.fget(bridge) is False
    assert AppBridge.onboardingRequired.fget(bridge) is False
    restored = AppSettings.from_dict(controller.state.settings.to_dict())
    assert restored.onboarding_version == constants.APP_VERSION


def test_complete_onboarding_never_moves_version_backwards() -> None:
    settings = AppSettings(onboarding_revision=constants.ONBOARDING_REVISION, onboarding_version="99.0.0")
    controller = _Controller(settings)
    bridge = _bridge(controller)
    assert AppBridge.onboardingRequired.fget(bridge) is False
    AppBridge.completeOnboarding(bridge)
    assert controller.updates == []
    assert controller.state.settings.onboarding_version == "99.0.0"


def test_fresh_profile_round_trip_stays_fresh() -> None:
    restored = AppSettings.from_dict(AppSettings().to_dict())
    assert restored.onboarding_version == ""
    assert AppSettings.from_dict({"onboarding_version": "garbage"}).onboarding_version == "0"


def test_overlay_is_flat_and_language_comes_first() -> None:
    overlay = (QML_DIR / "OnboardingOverlay.qml").read_text(encoding="utf-8")
    assert "Gradient" not in overlay
    assert "glow" not in overlay
    assert "clip: true\n        opacity: root.cardLevel" not in overlay
    assert "id: cardBorder" in overlay
    assert "blurMax: 128" in overlay
    assert "App.onboardingShowWhatsNew" in overlay
    assert "App.onboardingWhatsNew" in overlay
    news = overlay.index("id: newsPage")
    welcome = overlay.index("id: welcomePage")
    look = overlay.index("id: lookPage")
    route = overlay.index("id: routePage")
    assert overlay.count("LanguagePicker {") == 2
    # Fresh installs choose the language on the first page; the update notes
    # page has no picker, upgraded profiles find it on the look page.
    assert "LanguagePicker {" not in overlay[news:welcome]
    assert "LanguagePicker {" in overlay[welcome:look]
    look_block = overlay[look:route]
    assert "LanguagePicker {" in look_block
    assert 'visible: root.flow[0] !== "welcome"' in look_block
    welcome_block = overlay[welcome:look]
    assert "LumenMark" not in welcome_block
    done_block = overlay[overlay.index("id: donePage"):]
    assert 'I18n.t("Серверы")' not in done_block
    assert "warp-generation.github.io" not in overlay


def test_overlay_inputs_cards_and_support_block() -> None:
    overlay = (QML_DIR / "OnboardingOverlay.qml").read_text(encoding="utf-8")

    # Subscription link uses the same filled search-box style as the Servers page.
    assert "SearchField {\n                                                id: subField" in overlay
    # Cards of one group share the tallest height in every language.
    assert "function tallest(items)" in overlay
    assert overlay.count("root.tallest(children)") >= 4
    # The gap above a revealed panel animates with it (no jump at the end).
    assert "gap: 16" in overlay
    assert "implicitHeight: (holder.childrenRect.height + gap) * level" in overlay
    # The settings hint is a single line.
    assert "wrapMode: Text.NoWrap" in overlay and "fontSizeMode: Text.HorizontalFit" in overlay
    # Last page asks for a GitHub star and offers support.
    assert "SupportCard {" in overlay
    assert "https://github.com/krambovic/Lumen" in overlay
    assert "App.copyText(sup.usdtAddress)" in overlay


def test_onboarding_motion_always_reaches_visible_state() -> None:
    overlay = (QML_DIR / "OnboardingOverlay.qml").read_text(encoding="utf-8")
    main = (QML_DIR / "Main.qml").read_text(encoding="utf-8")

    # Render-thread Animators started before the window was exposed could
    # stay at opacity 0 on some drivers (empty window). Use property
    # animations plus a guard that forces the final visible state.
    for animator in ("OpacityAnimator", "ScaleAnimator", "YAnimator", "XAnimator"):
        assert animator not in overlay
        assert f"{animator} {{ target: appChrome" not in main
    assert 'NumberAnimation { target: cardLayer; property: "opacity"; from: 0; to: 1' in overlay
    assert 'NumberAnimation { target: backdrop; property: "opacity"; from: 0; to: 1' in overlay
    assert "id: revealGuard" in overlay and "revealGuard.restart()" in overlay
    assert "function settleCard()" in overlay and "onFinished: root.settleCard()" in overlay
    assert "cardLayer.opacity = 1" in overlay
    assert "live: !root.closing" in overlay
    assert 'NumberAnimation { target: appChrome; property: "opacity"' in main
    assert "appChrome.opacity = 1" in main
    assert "win.chromeSettle" not in main


def test_213_upgrade_shows_curated_news_once_even_after_completed_setup():
    from xray_fluent.release_notes import RELEASE_NOTES
    for previous in ("0", "1.9.18", "2.0.0", "2.1.0", "2.1.1", "2.1.2", "2.1.3-pre.01"):
        controller = _Controller(AppSettings(
            onboarding_revision=constants.ONBOARDING_REVISION,
            onboarding_version=previous,
        ))
        bridge = _bridge(controller)
        assert AppBridge.onboardingSetupRequired.fget(bridge) is False
        assert AppBridge.onboardingRequired.fget(bridge) is True
        assert AppBridge.onboardingShowWhatsNew.fget(bridge) is True
        groups = AppBridge.onboardingWhatsNew.fget(bridge)
        assert groups[0]["version"] == "2.1.3"
        assert groups[0]["items"] == [dict(item) for item in RELEASE_NOTES["2.1.3"]]
        AppBridge.completeOnboarding(bridge)
        restored = _bridge(_Controller(AppSettings.from_dict(controller.state.settings.to_dict())))
        assert AppBridge.onboardingRequired.fget(restored) is False
