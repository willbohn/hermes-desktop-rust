"""Isolation contracts for the developer-only real-WebKit smoke helper."""
import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/run-isolated-native.py"
spec = importlib.util.spec_from_file_location("native_smoke", SCRIPT)
assert spec is not None and spec.loader is not None
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class NativeIsolationTests(unittest.TestCase):
    def test_provider_credentials_and_live_profile_are_not_inherited(self):
        env = smoke.build_environment(Path("/disposable/profile"), 19222, {
            "HOME": "/real-home", "PATH": "/usr/bin", "DISPLAY": ":0",
            "OPENAI_API_KEY": "fake-test-key", "HERMES_HOME": "/real-home/.hermes",
            "HERMES_GATEWAY_LOCK_DIR": "/real-home/locks", "HERMES_PROFILE": "live",
        })
        self.assertEqual(env["PATH"], "/usr/bin")
        self.assertEqual(env["DISPLAY"], ":0")
        for key in ("OPENAI_API_KEY", "HERMES_HOME", "HERMES_PROFILE"):
            self.assertNotIn(key, env)
        self.assertNotIn("/real-home", " ".join(env.values()))

    def test_os_home_and_host_rendezvous_are_scoped_with_app_data(self):
        env = smoke.build_environment(Path("/disposable/profile"), 19222, {})
        state = Path("/disposable/native-smoke-state")
        for key in ("HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
                    "XDG_STATE_HOME", "HERMES_GATEWAY_LOCK_DIR"):
            self.assertTrue(Path(env[key]).is_relative_to(state), key)

    def test_debug_inspector_is_loopback_only(self):
        env = smoke.build_environment(Path("/disposable/profile"), 19222, {})
        self.assertEqual(env["WEBKIT_INSPECTOR_HTTP_SERVER"], "127.0.0.1:19222")
        self.assertEqual(env["WEBKIT_INSPECTOR_SERVER"], "127.0.0.1:19223")


if __name__ == "__main__":
    unittest.main()
