from __future__ import annotations

import logging
from typing import TYPE_CHECKING

import pyscreenshot as ImageGrab
from cv2 import (CHAIN_APPROX_SIMPLE, COLOR_BGR2GRAY, RETR_LIST, Canny,
                 boundingRect, cvtColor, dilate, findContours)
from numpy import array, ones, uint8

from hints.backends.backend import HintsBackend
from hints.backends.exceptions import AccessibleChildrenNotFoundError
from hints.child import Child

if TYPE_CHECKING:
    from PIL.Image import Image

logger = logging.getLogger(__name__)


class OpenCV(HintsBackend):
    """OpenCV Hints Backend."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.backend_name = "opencv"

    def screenshot(
        self,
        window_extents: tuple[int, int, int, int],
        window_extents_offsets: tuple[int, int, int, int] = (0, 0, 0, 0),
    ) -> Image:
        """Take a screenshot of a window specified by its extents.

        :param window_extents: The extents of a window to screenshot
            (x,y,width,height).
        :param window_extents_offsets: Any offsets for the screenshot
            area (window) (x,y,width,height).
        :return: Screeshot image.
        """
        x, y, w, h = window_extents
        return ImageGrab.grab(
            (
                x + window_extents_offsets[0],
                y + window_extents_offsets[1],
                x + w + window_extents_offsets[2],
                y + h + window_extents_offsets[3],
            ),
            backend="grim",
        )

    def get_children(self) -> list[Child]:
        """Get children.

        :return: Children.
        """
        children: list[Child] = []
        application_rules = self.get_application_rules()

        gray_image = cvtColor(
            array(self.screenshot(self.window_system.focused_window_extents)),
            COLOR_BGR2GRAY,
        )

        edges = Canny(
            gray_image,
            application_rules["canny_min_val"],
            application_rules["canny_max_val"],
        )

        kernel = ones(
            (application_rules["kernel_size"], application_rules["kernel_size"]), uint8
        )

        dilated_edges = dilate(edges, kernel)

        contours, _ = findContours(dilated_edges, RETR_LIST, CHAIN_APPROX_SIMPLE)

        for contour in contours:
            x, y, w, h = boundingRect(contour)
            children.append(
                Child(
                    absolute_position=(
                        x + self.window_system.focused_window_extents[0],
                        y + self.window_system.focused_window_extents[1],
                    ),
                    relative_position=(x, y),
                    width=w,
                    height=h,
                )
            )

        logger.debug(
            "Finished gathering hints for '%s'",
            self.window_system.focused_applicaiton_name,
        )

        if not children:
            raise AccessibleChildrenNotFoundError(
                self.window_system.focused_applicaiton_name
            )

        return children
