"""Mouse service for hints.

This service is an independent application that hints calls using a Unix
Domain Socket to perform mouse movements by writing to uinput. We use
custom uinput devices to support X11 and Wayland. This is separate from
the main hints application to prevent slowing down the main hints
process when creating virutal devices.
"""

from __future__ import annotations

import json
import logging
import os
import socket
import subprocess
from os import path, remove
from pickle import dumps, loads
from signal import SIGINT, signal
from time import sleep, time
from typing import TYPE_CHECKING, Any, Iterable

logger = logging.getLogger(__name__)

from evdev import AbsInfo, UInput, ecodes
from gi import require_version

from hints.constants import SOCKET_MESSAGE_SIZE, UNIX_DOMAIN_SOCKET_FILE
from hints.mouse_enums import MouseButton, MouseMode
from hints.utils import load_config

require_version("Gdk", "3.0")
require_version("Gtk", "3.0")
from gi.repository import Gdk, GLib, Gtk

if TYPE_CHECKING:
    from hints.mouse_enums import MouseButtonState

MOUSE_SERVICE_LOOP_MS_INTERVAL = 10
config = load_config()


class Mouse:
    """Mouse class for performing mouse actions (click, hover, move, etc).

    This uses uinput to support both X11 and Wayland.
    """

    def __init__(self, abs_max_width=10000, abs_max_height=10000, write_pause=0.03):

        keys = [button.value for button in MouseButton]
        self.write_pause = write_pause

        self.relative_mouse = UInput(
            {
                ecodes.EV_KEY: keys,
                ecodes.EV_REL: [
                    ecodes.REL_X,
                    ecodes.REL_Y,
                    ecodes.REL_HWHEEL,
                    ecodes.REL_WHEEL,
                ],
            },
            name="Hints relative mouse",
        )

        self.absolute_mouse = UInput(
            {
                ecodes.EV_KEY: keys,
                ecodes.EV_ABS: [
                    (
                        ecodes.ABS_X,
                        AbsInfo(
                            value=0,
                            min=0,
                            max=abs_max_width,
                            fuzz=0,
                            flat=0,
                            resolution=0,
                        ),
                    ),
                    (
                        ecodes.ABS_Y,
                        AbsInfo(
                            value=0,
                            min=0,
                            max=abs_max_height,
                            fuzz=0,
                            flat=0,
                            resolution=0,
                        ),
                    ),
                ],
            },
            name="Hints absolute mouse",
        )

    def scroll(self, x: int, y: int, *_args, **_kwargs):
        """Scroll event.

        :param x: X scroll direction.
        :param y: Y scroll direction. :param *_args: Extra args to use
            the same interface as move. :param **_kwargs: Extra kwargs
            to use the same interface as move.
        """
        self.relative_mouse.write(ecodes.EV_REL, ecodes.REL_HWHEEL, int(x))
        self.relative_mouse.write(ecodes.EV_REL, ecodes.REL_WHEEL, int(y))
        self.relative_mouse.syn()

    def move(self, x: int, y: int, absolute: bool = True):
        """Move event.

        :param X: X move direction.
        :param y: Y move direction.
        :param absolute: Whether to move the mouse using an absolute
            position.
        """

        if absolute:
            self.absolute_mouse.write(ecodes.EV_ABS, ecodes.ABS_X, int(x))
            self.absolute_mouse.write(ecodes.EV_ABS, ecodes.ABS_Y, int(y))
            self.absolute_mouse.syn()

        else:
            self.relative_mouse.write(ecodes.EV_REL, ecodes.REL_X, int(x))
            self.relative_mouse.write(ecodes.EV_REL, ecodes.REL_Y, int(y))
            self.relative_mouse.syn()

        sleep(self.write_pause)

    def click(
        self,
        x: int,
        y: int,
        button: MouseButton,
        button_states: Iterable[MouseButtonState],
        repeat: int = 1,
        absolute: bool = True,
    ):
        """Click event.

        :param x: X position to click.
        :param y: Y position to click.
        :param button: Button to use for click.
        :param actions: Actions to use for the click button (button down
            / button up).
        :param repeat: Times to repeat a click.
        :param absolute: Whether the click position is absolute.
        """
        self.move(x, y, absolute=absolute)

        device = self.absolute_mouse if absolute else self.relative_mouse
        for _ in range(repeat):
            for button_state in button_states:
                device.write(ecodes.EV_KEY, button, button_state)
                device.syn()
                sleep(self.write_pause)

        if absolute:
            # small move to clear previous write incase the previous move wants
            # to be repeated
            self.move(x + 1, y, absolute=True)
            self.move(x - 1, y, absolute=True)

    def do_mouse_action(
        self,
        key_press_state: dict[str, Any],
        key: str,
        mode: MouseMode,
    ):
        """Perform mouse action.

        :param key_press_state: State containing key press event data
            used for ramping up speeds.
        :param key: The key to perform a mouse action for.
        :param mode: The mouse mode.
        """
        key_press_state.setdefault("start_time", time())

        sensitivity = 1
        rampup_time = 1
        mouse_navigation_action = self.move
        left = "h"
        right = "l"
        up = "k"
        down = "j"

        if mode == MouseMode.MOVE.value:
            sensitivity = config["mouse_move_pixel_sensitivity"]
            rampup_time = config["mouse_move_rampup_time"]
            left = config["mouse_move_left"]
            right = config["mouse_move_right"]

            # up and down are intentionally switched to keep the logic the same
            # as scrol
            up = config["mouse_move_down"]
            down = config["mouse_move_up"]

            mouse_navigation_action = self.move

        elif mode == MouseMode.SCROLL.value:
            sensitivity = config["mouse_scroll_pixel_sensitivity"]
            rampup_time = config["mouse_scroll_rampup_time"]
            left = config["mouse_scroll_left"]
            right = config["mouse_scroll_right"]
            up = config["mouse_scroll_up"]
            down = config["mouse_scroll_down"]
            mouse_navigation_action = self.scroll

        key_press_state.setdefault("sensitivity", sensitivity)

        if time() - key_press_state["start_time"] >= rampup_time:
            key_press_state["sensitivity"] += sensitivity

        if key == left:
            mouse_navigation_action(-key_press_state["sensitivity"], 0, absolute=False)
        if key == right:
            mouse_navigation_action(key_press_state["sensitivity"], 0, absolute=False)
        if key == up:
            mouse_navigation_action(0, key_press_state["sensitivity"], absolute=False)
        if key == down:
            mouse_navigation_action(0, -key_press_state["sensitivity"], absolute=False)

        return key_press_state


class MouseService:
    """Mouse Service.

    This is responsible for running the mouse service and detecting
    events requring the mouse devices to reload / be updated.
    """

    def __init__(self):
        """Mouse Service Constructor."""
        Gtk.init()

        self.screen = Gdk.Screen.get_default()
        self._region = self._get_total_display_region()
        self.mouse = Mouse(self._region[2], self._region[3])
        self._apply_sway_mapping()

        if path.exists(UNIX_DOMAIN_SOCKET_FILE):
            remove(UNIX_DOMAIN_SOCKET_FILE)

        self.socket = socket.socket(
            socket.AF_UNIX, socket.SOCK_STREAM | socket.SOCK_NONBLOCK
        )
        self.socket.bind(UNIX_DOMAIN_SOCKET_FILE)
        self.socket.listen(1)
        GLib.timeout_add(MOUSE_SERVICE_LOOP_MS_INTERVAL, self.socket_connection)

        self.screen.connect("size-changed", self.on_size_changed)

        display = Gdk.Display.get_default()
        display.connect("monitor-added", self.on_monitors_changed)
        display.connect("monitor-removed", self.on_monitors_changed)

        signal(SIGINT, self.on_interrupt)

    def on_interrupt(self, *_):
        """Interrupt handler to clean up."""
        self.socket.close()
        Gtk.main_quit()

    def _get_total_display_region(self) -> tuple[int, int, int, int]:
        """Get the bounding box (x, y, width, height) spanning all monitors.

        Uses min/max of monitor geometries so negative-offset monitors work.
        """
        display = Gdk.Display.get_default()
        n = display.get_n_monitors()
        if n == 0:
            return (0, 0, self.screen.get_width(), self.screen.get_height())
        g0 = display.get_monitor(0).get_geometry()
        min_x, min_y = g0.x, g0.y
        max_x, max_y = g0.x + g0.width, g0.y + g0.height
        for i in range(1, n):
            g = display.get_monitor(i).get_geometry()
            min_x = min(min_x, g.x)
            min_y = min(min_y, g.y)
            max_x = max(max_x, g.x + g.width)
            max_y = max(max_y, g.y + g.height)
        return (min_x, min_y, max_x - min_x, max_y - min_y)

    def _apply_sway_mapping(self, attempts: int = 8):
        """On sway, map the Hints absolute uinput device to span all outputs.

        Sway assigns virtual absolute pointers to a single output by default,
        which causes clicks to land on the wrong monitor in multi-monitor
        setups. `swaymsg input <id> map_to_region` remaps to the full span.
        Retries because uinput device registration is async.
        """
        if not os.environ.get("SWAYSOCK"):
            return
        x, y, w, h = self._region

        def try_map():
            try:
                out = subprocess.run(
                    ["swaymsg", "-t", "get_inputs", "-r"],
                    capture_output=True, check=True, timeout=2,
                )
                inputs = json.loads(out.stdout.decode("utf-8"))
            except Exception as exc:
                logger.debug("swaymsg get_inputs failed: %s", exc)
                return True  # retry
            for dev in inputs:
                name = dev.get("name", "")
                if "Hints absolute mouse" not in name:
                    continue
                ident = dev.get("identifier")
                if not ident:
                    continue
                try:
                    subprocess.run(
                        ["swaymsg", "input", ident,
                         "map_to_region", str(x), str(y), str(w), str(h)],
                        capture_output=True, check=True, timeout=2,
                    )
                    logger.debug(
                        "Mapped sway input %s to region %d,%d %dx%d",
                        ident, x, y, w, h,
                    )
                except Exception as exc:
                    logger.warning("swaymsg map_to_region failed: %s", exc)
                return False  # done
            return True  # device not found yet, retry

        remaining = [attempts]

        def tick():
            remaining[0] -= 1
            keep_going = try_map()
            return keep_going and remaining[0] > 0

        GLib.timeout_add(250, tick)

    def _reload_mouse(self):
        self._region = self._get_total_display_region()
        self.mouse = Mouse(self._region[2], self._region[3])
        self._apply_sway_mapping()

    def on_size_changed(self, screen: Gdk.Screen):
        """Screen size change event handler to update the mouse device min/max
        values for correct absolute position movement.

        :param screen: The screen object for the event.
        """
        self._reload_mouse()

    def on_monitors_changed(self, _display: Gdk.Display, _monitor: Gdk.Monitor):
        """Monitor add/remove event handler to update the mouse device min/max
        values. Gdk.Screen size-changed is not reliably emitted on Wayland when
        outputs are hotplugged or configured after startup (e.g. on sway login).

        :param display: The display object for the event.
        :param monitor: The monitor that was added or removed.
        """
        self._reload_mouse()

    def socket_connection(self):
        """Handle socket connection events.

        This is how the main hints process and the mouse service
        communicate.
        """
        try:
            connection, _ = self.socket.accept()
            payload = loads(connection.recv(SOCKET_MESSAGE_SIZE))
            method = payload.get("method", "")
            args = payload.get("args", ())
            kwargs = payload.get("kwargs", {})
            connection.send(
                dumps(
                    {
                        "click": self.mouse.click,
                        "move": self.mouse.move,
                        "scroll": self.mouse.scroll,
                        "do_mouse_action": self.mouse.do_mouse_action,
                    }[method](*args, **kwargs)
                )
            )
        except BlockingIOError:
            pass

        return GLib.SOURCE_CONTINUE

    def run(self):
        """Run the mouse service."""
        Gtk.main()


def main():
    """Mouse service entry point."""
    MouseService().run()


if __name__ == "__main__":
    main()
