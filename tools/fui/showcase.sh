#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/showcase.sh -- THE fUi SHOWCASE: every control in every state,
# light and dark, painted headless, diffed against the checked-in goldens.
#
# The showcase is the consolidated set of the gallery programs
# (gallery = buttons/labels/grid, gallery2 = every wave-2 control in all
# five states, gallery3 = lists/tables/cards/dialogs, gallery4 = text,
# gallery9 = a whole described page). Each paints into a PNG without a
# window; tools/fui/refdiff.py compares the pixels with tools/fui/golden/
# and prints ONE number per image: the share of differing pixels.
#
#     tools/fui/showcase.sh                 compare against the goldens
#     tools/fui/showcase.sh --update        a deliberate design change:
#                                           take the new images as goldens
#     tools/fui/showcase.sh --out DIR       also keep the images in DIR
#
# Exit code 1 if an image is missing, has another size, or differs by more
# than MAX percent (default 0.5, env SHOWCASE_MAX).
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
FONT_TTF="${FONT_TTF:-/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf}"
MAX="${SHOWCASE_MAX:-0.5}"
MODE=compare
KEEP=""
while [ $# -gt 0 ]; do
    case "$1" in
        --update) MODE=update ;;
        --out) KEEP="$2"; shift ;;
        *) echo "showcase: unknown option $1"; exit 2 ;;
    esac
    shift
done

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/img"

for g in gallery gallery2 gallery3 gallery4 gallery9; do
    "$FIRNC" --opt-level=dev -o "$T/$g" "tools/fui/${g}_main.fi"
    for scheme in light dark; do
        if [ "$g" = gallery9 ]; then
            "$T/$g" "$T/img/$g-$scheme.png" "$scheme" - 1240 > /dev/null
        else
            "$T/$g" "$T/img/$g-$scheme.png" "$scheme" "$FONT_TTF" > /dev/null
        fi
    done
done

# GLASS (lib/fui/glass.fi): off / subtle / strong, liquid + acrylic + mica
"$FIRNC" --opt-level=release-fast -o "$T/glass" tools/fui/glass_main.fi
for scheme in light dark; do
    "$T/glass" shot "$scheme" "$T/img/glass-$scheme.png"
done

# GLASS ON THE SURFACES (lib/fui/glasspane.fi): title bar + tooltip (a), modal (b), levels 0/1/2
"$FIRNC" --opt-level=dev -o "$T/glasspane" tools/fui/glasspane_main.fi
"$T/glasspane" shot "$T/img" > /dev/null

# WAVE 5 (lib/fui/wave5.fi): rating, chips, segmented, skeleton, chart, page dots, pager -- light, dark, RTL
"$FIRNC" --opt-level=dev -o "$T/wave5" tools/fui/wave5_main.fi
"$T/wave5" "$T/img" > /dev/null

# WAVE 6 (lib/fui/wave6.fi): property grid + half-open modal bottom sheet with a grid as body -- light, dark
"$FIRNC" --opt-level=dev -o "$T/wave6" tools/fui/wave6_main.fi
"$T/wave6" "$T/img" > /dev/null

# WAVE 7 (lib/fui/wave7.fi): range slider -- light, dark
"$FIRNC" --opt-level=dev -o "$T/wave7" tools/fui/wave7_main.fi
"$T/wave7" "$T/img" > /dev/null

# WAVE 8 (lib/fui/wave8.fi): spinner, time picker, breadcrumb, info bar, stepper -- light, dark
"$FIRNC" --opt-level=dev -o "$T/wave8" tools/fui/wave8_main.fi
"$T/wave8" "$T/img" > /dev/null

# GRID in the ten-line API (appctl.grid): a form with spans, light, dark
"$FIRNC" --opt-level=dev -o "$T/appgrid" tools/fui/appgrid_main.fi
"$T/appgrid" "$T/img" > /dev/null

# DRAG AND DROP (appctl.dnd_*): the drop marker between rows and the ghost at the pointer, light, dark
"$FIRNC" --opt-level=dev -o "$T/appw10" tools/fui/appw10_main.fi
"$T/appw10" "$T/img" > /dev/null

# TOOLBAR OVERFLOW (appctl.toolbar): narrow bar with "..." and the open panel, light, dark
"$FIRNC" --opt-level=dev -o "$T/apptb" tools/fui/apptb_main.fi
"$T/apptb" "$T/img" > /dev/null

# TABLE (appctl.table): header sort arrow and the open cell editor, light, dark
"$FIRNC" --opt-level=dev -o "$T/appw11" tools/fui/appw11_main.fi
"$T/appw11" "$T/img" > /dev/null

# WAVE 12 (appctl.datepicker / navview / tree): calendar range, expanded navigation pane, open tree, light, dark
"$FIRNC" --opt-level=dev -o "$T/appw12" tools/fui/appw12_main.fi
"$T/appw12" "$T/img" > /dev/null

# WAVE 13 (appctl.flyout / teaching_tip): flyout below its anchor, tip flipped below, light, dark
"$FIRNC" --opt-level=dev -o "$T/appw13" tools/fui/appw13_main.fi
"$T/appw13" "$T/img" > /dev/null

# MOBILE TOKENS (webtheme): type steps, palette swatches, radii, shadows, bubble gradient, light, dark
"$FIRNC" --opt-level=dev -o "$T/webtheme" tools/fui/webtheme_main.fi
"$T/webtheme" "$T/img" > /dev/null

# MOBILE COMPONENTS (lib/fui/mobile.fi): every component once on a 360x1200 page, dark + light
"$FIRNC" --opt-level=dev -o "$T/mobile" tools/fui/mobile_main.fi
"$T/mobile" "$T/img" > /dev/null

if [ -n "$KEEP" ]; then
    mkdir -p "$KEEP"
    cp "$T"/img/*.png "$KEEP"/
fi

if [ "$MODE" = update ]; then
    python3 -I tools/fui/refdiff.py update "$T/img" tools/fui/golden
else
    python3 -I tools/fui/refdiff.py compare "$T/img" tools/fui/golden --max "$MAX"
fi
