"""Sway window system."""

from json import loads
from subprocess import run

from hints.window_systems.window_system import WindowSystem


class Hyprland(WindowSystem):
    """Hyprland Window system class."""

    def __init__(self):
        super().__init__()
        self._snapshot = None

    def _get_snapshot(self) -> dict:
        if self._snapshot is None:
            result = run(
                ["hyprctl", "activewindow", "-j"], capture_output=True, check=True
            )
            self._snapshot = loads(result.stdout.decode("utf-8"))
        return self._snapshot  # type: ignore[return-value]

    @property
    def window_system_name(self) -> str:
        """Get the name of the window syste.

        This is useful for performing logic specific to a window system.

        :return: The window system name
        """
        return "Hyprland"

    @property
    def focused_window_extents(self) -> tuple[int, int, int, int]:
        """Get active window extents.

        :return: Active window extents (x, y, width, height).
        """
        snap = self._get_snapshot()
        x, y = snap["at"]
        width, height = snap["size"]
        return (x, y, width, height)

    @property
    def focused_window_pid(self) -> int:
        """Get Process ID corresponding to the focused widnow.

        :return: Process ID of focused window.
        """
        return self._get_snapshot()["pid"]

    @property
    def focused_applicaiton_name(self) -> str:
        """Get focused application name.

        This name is the name used to identify applications for per-
        application rules.

        :return: Focused application name.
        """
        return self._get_snapshot()["class"]
