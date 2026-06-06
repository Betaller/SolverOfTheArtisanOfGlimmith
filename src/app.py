from __future__ import annotations

import os
import sys

# 确保项目根目录在 Python 路径中
_project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

from PySide6.QtWidgets import QApplication
from PySide6.QtCore import Qt

from src.ui.main_window import MainWindow
from src.ui.theme import apply_theme


def _is_system_dark(app: QApplication) -> bool:
    """Detect Windows dark mode or Qt 6.5+ QStyleHints.colorScheme."""
    try:
        scheme = app.styleHints().colorScheme()
        return scheme == Qt.ColorScheme.Dark
    except AttributeError:
        pass
    # Fallback for older Qt: check Windows registry
    try:
        import winreg
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
        )
        value, _ = winreg.QueryValueEx(key, "AppsUseLightTheme")
        return value == 0
    except Exception:
        return False


def main() -> None:
    app = QApplication(sys.argv)
    app.setApplicationName("格里米斯的工匠 - 求解器")
    app.setOrganizationName("TAGSolver")

    dark = _is_system_dark(app)
    apply_theme(app, dark)

    window = MainWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
