"""Sway window system."""

from json import loads
from subprocess import PIPE, Popen

from hints.window_systems.window_system import WindowSystem


class Sway(WindowSystem):
    """Sway Window system class."""

    def __init__(self):
        super().__init__()
        self._snapshot = None

    def _fetch_snapshot(self):
        """Fetch all sway state in one go for consistency."""
        swaytree = Popen(["swaymsg", "-t", "get_tree"], stdout=PIPE)
        focused_window = Popen(
            ["jq", ".. | select(.type?) | select(.focused==true)"],
            stdin=swaytree.stdout,
            stdout=PIPE,
        )
        window = loads(focused_window.communicate()[0].decode("utf-8"))

        swaytree = Popen(["swaymsg", "-t", "get_workspaces"], stdout=PIPE)
        focused_ws = Popen(
            ["jq", ".[] | select(.focused==true)"],
            stdin=swaytree.stdout,
            stdout=PIPE,
        )
        workspace = loads(focused_ws.communicate()[0].decode("utf-8"))

        swaytree = Popen(["swaymsg", "-t", "get_outputs"], stdout=PIPE)
        focused_out = Popen(
            ["jq", ".[] | select(.focused==true)"],
            stdin=swaytree.stdout,
            stdout=PIPE,
        )
        output = loads(focused_out.communicate()[0].decode("utf-8"))

        self._snapshot = {
            "window": window,
            "workspace": workspace,
            "output": output,
        }

    def _get_snapshot(self) -> dict:
        if self._snapshot is None:
            self._fetch_snapshot()
        return self._snapshot  # type: ignore[return-value]

    @property
    def bar_height(self) -> int:
        snap = self._get_snapshot()
        return (
            snap["output"]["rect"]["height"]
            - snap["workspace"]["rect"]["height"]
        )

    @property
    def window_system_name(self) -> str:
        """Get the name of the window syste.

        This is useful for performing logic specific to a window system.

        :return: The window system name
        """
        return "sway"

    @property
    def focused_window_extents(self) -> tuple[int, int, int, int]:
        """Get active window extents.

        Skips the sway window decoration (title bar) so the screenshot
        and overlay start at the actual application content area.

        :return: Active window extents (x, y, width, height).
        """
        focused_window = self._get_snapshot()["window"]
        rect = focused_window["rect"]
        deco_height = focused_window.get("deco_rect", {}).get("height", 0)
        return (
            rect["x"],
            rect["y"] + deco_height,
            rect["width"],
            rect["height"] - deco_height,
        )

    @property
    def focused_window_pid(self) -> int:
        """Get Process ID corresponding to the focused widnow.

        :return: Process ID of focused window.
        """
        return self._get_snapshot()["window"]["pid"]

    @property
    def focused_applicaiton_name(self) -> str:
        """Get focused application name.

        This name is the name used to identify applications for per-
        application rules.

        :return: Focused application name.
        """
        return self._get_snapshot()["window"]["app_id"]
