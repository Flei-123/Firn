#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
# tools/fui/rename.py -- fUi VON DEUTSCHEN AUF ENGLISCHE BEZEICHNER.
#
# Die Umstellung ist MECHANISCH: eine Tabelle alt->neu, angewandt mit
# Wortgrenzen, auf jede .fi-Datei und jedes Werkzeug. Kein Schritt von
# Hand -- eine Umbenennung von Hand ist genau die Gelegenheit, bei der
# ein Aufrufer uebersehen wird.
#
# DIE REGEL: nur BEZEICHNER. Kommentare und Dokumentation bleiben
# deutsch (Justin liest sie). Darum wird mit Wortgrenzen ersetzt und
# NICHT auf Kommentarzeilen verzichtet -- ein Bezeichner, der im
# Kommentar erklaert wird, soll dort auch seinen neuen Namen tragen.
#
# Die Namen sind die UEBLICHEN aus echten UI-Bibliotheken (Qt, GTK,
# Flutter, SwiftUI), nicht Wort-fuer-Wort aus dem Deutschen:
#   Zeichner -> Painter (nicht "Drawer")
#   Marke    -> Theme   (nicht "Brand" -- es IST ein Theme)
#   Satz     -> StyleSet
#   Ueber    -> Override
#   Kasten   -> Rect
#   Stapel   -> Box     (Flexbox-Box: die uebliche Bezeichnung)
import re
import sys
import os

# ======================================================================
# DIE MODULE (Dateinamen und Importpfade)
# ======================================================================
#
# Der Schnitt bleibt, nur die Namen aendern sich. `male` -> `paint`
# waere eine Kollision mit dem vorhandenen `paint.canvas`, darum
# `render`: es ist die Datei, die aus einem Widget Bildpunkte macht.
MODULE = {
    "kern":       "core",
    "stil":       "style",
    "marke":      "theme",
    "zeichnen":   "painter",
    "element":    "widget",
    "male":       "render",
    "anordnung":  "layout",
    "textfeld":   "textbuf",
    "edit":       "editor",
    "symbol":     "icon",
}

# ======================================================================
# DIE BEZEICHNER
# ======================================================================
#
# Reihenfolge egal: ersetzt wird mit Wortgrenzen, und kein Name ist
# Praefix eines anderen in einer Weise, die das stoert (dafuer sorgt
# \b).
NAMES = {
    # ---------------------------------------------------- fui.style
    "Stil": "Style",
    "stil_neu": "style_new",
    "stil_leer": "style_empty",
    "Ueber": "Override",
    "ueber_neu": "override_new",
    "ueber_leer": "override_empty",
    "Satz": "StyleSet",
    "satz_neu": "styleset_new",
    "satz_stil": "styleset_base",
    "satz_ueber": "styleset_override",
    "satz_setz_ueber": "styleset_set_override",
    "farb_literal": "color_literal",
    "farb_token": "color_token",
    "farb_transparent": "color_transparent",
    "farb_erben": "color_inherit",
    "farb_art": "color_kind",
    "farb_wert": "color_value",
    "farb_ist_sichtbar": "color_is_visible",
    "farb_ist_token": "color_is_token",
    "ART_LITERAL": "COLOR_LITERAL",
    "ART_TOKEN": "COLOR_TOKEN",
    "ART_TRANSPARENT": "COLOR_TRANSPARENT",
    "ART_ERBEN": "COLOR_INHERIT",
    # Die Token. TK_ -> TOK_, und die Rollen englisch.
    "TK_GRUND": "TOK_BASE",
    "TK_FLAECHE": "TOK_SURFACE",
    "TK_FLAECHE_HOCH": "TOK_SURFACE_RAISED",
    "TK_KNOPF": "TOK_BUTTON",
    "TK_KNOPF_UEBER": "TOK_BUTTON_HOVER",
    "TK_KNOPF_DRUCK": "TOK_BUTTON_ACTIVE",
    "TK_FELD": "TOK_FIELD",
    "TK_FELD_FOKUS": "TOK_FIELD_FOCUS",
    "TK_LINIE": "TOK_BORDER",
    "TK_TRENNER": "TOK_SEPARATOR",
    "TK_TEXT": "TOK_TEXT",
    "TK_TEXT_BLASS": "TOK_TEXT_MUTED",
    "TK_TEXT_GESPERRT": "TOK_TEXT_DISABLED",
    "TK_TEXT_AUF_AKZENT": "TOK_TEXT_ON_ACCENT",
    "TK_AKZENT": "TOK_ACCENT",
    "TK_AUSWAHL": "TOK_SELECTION",
    "TK_AUSWAHL_TEXT": "TOK_SELECTION_TEXT",
    "TK_FEHLER": "TOK_ERROR",
    "TK_WARNUNG": "TOK_WARNING",
    "TK_GUT": "TOK_SUCCESS",
    "TK_ANZAHL": "TOK_COUNT",
    # Die Zustaende. ZU_ -> STATE_.
    "ZU_NORMAL": "STATE_NORMAL",
    "ZU_UEBER": "STATE_HOVER",
    "ZU_DRUCK": "STATE_ACTIVE",
    "ZU_FOKUS": "STATE_FOCUS",
    "ZU_SPERR": "STATE_DISABLED",
    "ZU_GEWAEHLT": "STATE_SELECTED",
    "ZU_ANZAHL": "STATE_COUNT",
    "AUS_LINKS": "ALIGN_LEFT",
    "AUS_MITTE": "ALIGN_CENTER",
    "AUS_RECHTS": "ALIGN_RIGHT",
    "GEW_LEICHT": "WEIGHT_LIGHT",
    "GEW_NORMAL": "WEIGHT_NORMAL",
    "GEW_FETT": "WEIGHT_BOLD",
    "SCHNITT_GERADE": "SLANT_UPRIGHT",
    "SCHNITT_KURSIV": "SLANT_ITALIC",
    # Die Bitmaske "welches Feld ist gesetzt".
    "S_HG": "SF_BG",
    "S_RAHMEN_ST": "SF_BORDER_W",
    "S_RAHMEN": "SF_BORDER",
    "S_TEXTFARBE": "SF_FG",
    "S_RADIUS": "SF_RADIUS",
    "S_PAD_X": "SF_PAD_X",
    "S_PAD_Y": "SF_PAD_Y",
    "S_FONT_PX": "SF_FONT_PX",
    "S_GEWICHT": "SF_WEIGHT",
    "S_SCHNITT": "SF_SLANT",
    "S_AUSRICHT": "SF_ALIGN",
    "S_DECKKRAFT": "SF_ALPHA",
    "S_FONT_NR": "SF_FONT_ID",
    "stil_setz_hg": "style_set_bg",
    "stil_setz_rahmen_st": "style_set_border_width",
    "stil_setz_rahmen": "style_set_border",
    "stil_setz_textfarbe": "style_set_fg",
    "stil_setz_radius": "style_set_radius",
    "stil_setz_pad": "style_set_pad",
    "stil_setz_font": "style_set_font",
    "stil_setz_gewicht": "style_set_weight",
    "stil_setz_schnitt": "style_set_slant",
    "stil_setz_ausricht": "style_set_align",
    "stil_setz_deckkraft": "style_set_alpha",
    "ueber_setz_hg": "override_set_bg",
    "ueber_setz_rahmen": "override_set_border",
    "ueber_setz_textfarbe": "override_set_fg",
    "ueber_setz_deckkraft": "override_set_alpha",
    "loese_hg": "resolve_bg",
    "loese_rahmen": "resolve_border",
    "loese_textfarbe": "resolve_fg",
    "loese_deckkraft": "resolve_alpha",
    "loese_zahl": "resolve_num",
    "loese_farbe": "resolve_color",
    # Felder von Stil/Ueber/Satz
    "hg": "bg",
    "rahmen_st": "border_w",
    "rahmen": "border",
    "textfarbe": "fg",
    "radius": "radius",
    "pad_x": "pad_x",
    "pad_y": "pad_y",
    "font_px": "font_px",
    "gewicht": "weight",
    "schnitt": "slant",
    "font_nr": "font_id",
    "ausricht": "align",
    "deckkraft": "alpha",
    "gesetzt": "set_mask",

    # ---------------------------------------------------- fui.theme
    "Marke": "Theme",
    "marke_neu": "theme_new",
    "marke_farben": "theme_colors",
    "marke_form": "theme_shape",
    "marke_ist_dunkel": "theme_is_dark",
    "marke_setz_dunkel": "theme_set_dark",
    "marke_setz_form": "theme_set_shape",
    "marke_setz_skala": "theme_set_scale",
    "marke_skala": "theme_scale",
    "marke_px": "theme_px",
    "Farben": "Colors",
    "farben_hell": "colors_light",
    "farben_dunkel": "colors_dark",
    "farben_von_dunkel": "colors_for_dark",
    "Form": "Shape",
    "form_osum": "shape_osum",
    "form_klassisch": "shape_classic",
    "flaeche_fuer": "surface_for",
    "text_fuer": "text_for",
    "rand_fuer": "border_for",
    "leucht": "luminance",
    "kontrast": "contrast",
    "deck": "opaque",
    "misch": "mix",
    "farbe_von_token": "color_from_token",
    "farbe_von_stilwert": "color_from_style",
    # Felder von Farben
    "grund": "base",
    "flaeche_hoch": "surface_raised",
    "flaeche": "surface",
    "knopf_ueber": "button_hover",
    "knopf_druck": "button_active",
    "knopf": "button",
    "feld_fokus": "field_focus",
    "feld": "field",
    "linie": "border_col",
    "trenner_kraft": "sep_strength",
    "trenner": "separator",
    "text_blass": "text_muted",
    "text_gesperrt": "text_disabled",
    "text_auf_akzent": "text_on_accent",
    "akzent": "accent",
    "auswahl_text": "selection_text",
    "auswahl": "selection",
    "fehler": "error",
    "warnung": "warning",
    "gut": "success",
    "dunkel": "dark",
    # Felder von Form
    "r_fenster": "r_window",
    "r_tafel": "r_panel",
    "r_knopf": "r_button",
    "r_feld": "r_field",
    "rand": "border_w",
    "fokus": "focus_w",
    "sp_xs": "sp_xs",
    "sp_s": "sp_s",
    "sp_m": "sp_m",
    "sp_l": "sp_l",
    "ctrl_h": "ctrl_h",
    "row": "row",
    "font": "font",
    "motion": "motion",
    "schatten_r": "shadow_r",
    "schatten": "shadow",
    "skala": "scale",

    # --------------------------------------------------- fui.painter
    "Zeichner": "Painter",
    "zeichner_neu": "painter_new",
    "zeichner_init": "painter_init",
    "zeichner_frei": "painter_free",
    "zeichner_ok": "painter_ok",
    "zeichner_leinwand": "painter_canvas",
    "zeichner_setz_schrift": "painter_set_font",
    "zeichner_schrift": "painter_font",
    "rund": "round_rect",
    "ring": "round_ring",
    "text_breite": "text_width",
    "text_passt_bis": "text_fits_upto",
    "text_mitte_y": "text_baseline_mid",
    "text_L": "TEXT_L",
    "text_M": "TEXT_M",
    "text_R": "TEXT_R",
    "marke_strich": "caret_bar",
    "hoehe_ascent": "font_ascent",
    "hoehe_descent": "font_descent",
    "pfad_rund_gegen": "path_round_ccw",
    "pfad_rund": "path_round",
    "fenster_auf": "raster_window",
    "utf8_at": "utf8_at",
    "klemm": "clamp",

    # ---------------------------------------------------- fui.widget
    "Element": "Widget",
    "element_neu": "widget_new",
    "el_lege": "w_place",
    "el_trifft": "w_hits",
    "el_setz_zustand": "w_set_state_bits",
    "el_zustand": "w_state",
    "el_setz_ueber": "w_set_hover",
    "el_setz_druck": "w_set_active",
    "el_setz_fokus": "w_set_focus",
    "el_setz_sperr": "w_set_disabled",
    "el_setz_gewaehlt": "w_set_selected",
    "el_ist_ueber": "w_is_hover",
    "el_ist_druck": "w_is_active",
    "el_ist_fokus": "w_is_focus",
    "el_ist_sperr": "w_is_disabled",
    "el_ist_gewaehlt": "w_is_selected",
    "el_ist_bedienbar": "w_is_enabled",
    "el_satz": "w_styleset",
    "el_setz_satz": "w_set_styleset",
    "el_stil": "w_style",
    "el_setz_stil": "w_set_style",
    "el_setz_text": "w_set_text",
    "el_text_len": "w_text_len",
    "el_text": "w_text",
    "el_setz_wert": "w_set_value",
    "el_wert": "w_value",
    "el_setz_kennung": "w_set_id",
    "el_kennung": "w_id",
    "el_setz_art": "w_set_kind",
    "el_art": "w_kind",
    "el_wunsch_w": "w_pref_w",
    "el_wunsch_h": "w_pref_h",
    "el_setz_wunsch": "w_set_pref",
    "el_setz_dehnbar": "w_set_grow",
    "el_dehnt_x": "w_grows_x",
    "el_dehnt_y": "w_grows_y",
    "el_x": "w_x",
    "el_y": "w_y",
    "el_w": "w_w",
    "el_h": "w_h",
    "setz_bit": "set_bit",
    # Die Arten. AR_ -> KIND_.
    "AR_LEER": "KIND_NONE",
    "AR_BESCHRIFTUNG": "KIND_LABEL",
    "AR_KNOPF": "KIND_BUTTON",
    "AR_UMSCHALT": "KIND_TOGGLE",
    "AR_ANKREUZ": "KIND_CHECKBOX",
    "AR_AUSWAHL": "KIND_RADIO",
    "AR_TEXTFELD": "KIND_TEXTBOX",
    "AR_SCHIEBER": "KIND_SLIDER",
    "AR_FORTSCHRITT": "KIND_PROGRESS",
    "AR_AUFKLAPP": "KIND_DROPDOWN",
    "AR_LISTE": "KIND_LIST",
    "AR_REITER": "KIND_TAB",
    "AR_WERKZEUG": "KIND_TOOLBAR",
    "AR_MENUE": "KIND_MENU",
    "AR_BILDLAUF": "KIND_SCROLLBAR",
    "AR_TRENNER": "KIND_SEPARATOR",
    "AR_GRUPPE": "KIND_GROUPBOX",
    "AR_TABELLE": "KIND_TABLE",
    "AR_BAUM": "KIND_TREE",
    "AR_KACHEL": "KIND_TILE",
    "AR_KARTE": "KIND_CARD",
    "AR_ABZEICHEN": "KIND_BADGE",
    "AR_HINWEIS": "KIND_TOOLTIP",
    "B_UEBER": "BIT_HOVER",
    "B_DRUCK": "BIT_ACTIVE",
    "B_FOKUS": "BIT_FOCUS",
    "B_SPERR": "BIT_DISABLED",
    "B_GEWAEHLT": "BIT_SELECTED",
    "art": "kind",
    "bits": "bits",
    "satz": "styleset",
    "wert": "value",
    "kennung": "id",
    "wunsch_w": "pref_w",
    "wunsch_h": "pref_h",
    "dehnt": "grow",

    # ---------------------------------------------------- fui.render
    "ctx_neu": "ctx_new",
    "ctx_marke": "ctx_theme",
    "ctx_zeichner": "ctx_painter",
    "male_grund": "draw_base",
    "male_element": "draw_widget",
    "male_beschriftung": "draw_label",
    "male_knopf": "draw_button",
    "male_fokusring": "draw_focus_ring",
    "male_text": "draw_text",
    "wid_hg_zu": "def_bg_state",
    "wid_hg": "def_bg",
    "wid_rahmen_st": "def_border_width",
    "wid_rahmen": "def_border",
    "wid_text": "def_fg",
    "wid_radius": "def_radius",
    "wid_pad_x": "def_pad_x",
    "wid_pad_y": "def_pad_y",
    "wid_font": "def_font",
    "wid_ausricht": "def_align",
    "farbe_von": "color_of",
    "laenge_von": "len_of",

    # ---------------------------------------------------- fui.layout
    "Kasten": "Rect",
    "kasten_neu": "rect_new",
    "kasten_x": "rect_x",
    "kasten_y": "rect_y",
    "kasten_w": "rect_w",
    "kasten_h": "rect_h",
    "kasten_schrumpf": "rect_shrink",
    "kasten_verschiebe": "rect_offset",
    "Stapel": "Box",
    "stapel_neu": "box_new",
    "stapel_setz_rand": "box_set_margin",
    "stapel_setz_abstand": "box_set_gap",
    "stapel_setz_ausricht": "box_set_align",
    "stapel_messen": "box_measure",
    "stapel_legen": "box_layout",
    "Raster": "Grid",
    "raster_neu": "grid_new",
    "raster_setz_rand": "grid_set_margin",
    "raster_setz_abstand": "grid_set_gap",
    "raster_zelle": "grid_cell",
    "verteile": "distribute",
    "mittig_in": "center_in",
    "ausrichten": "align_in",
    "WAAGRECHT": "HORIZONTAL",
    "SENKRECHT": "VERTICAL",
    "A_ANFANG": "ALIGN_START",
    "A_MITTE": "ALIGN_MID",
    "A_ENDE": "ALIGN_END",
    "A_DEHNEN": "ALIGN_STRETCH",
    "quer_ganz": "cross_size",
    "quer_pos": "cross_pos",
    "st_kasten": "store_rect",
    "abstand": "gap",
    "richtung": "dir",

    # ---------------------------------------------------- fui.core
    "Ziel": "Target",
    "ziel_neu": "target_new",
    "setz": "put",
    "hole": "get",
    "fuellen": "fill_all",
    "rechteck": "fill_rect",
    "linie_h": "line_h",
    "linie_v": "line_v",
    "rund_flaeche": "round_fill",
    "rund_rahmen": "round_frame",
    "ring_einer": "ring_one",
    "kn_minimieren": "cap_minimize",
    "kn_maximieren": "cap_maximize",
    "kn_wiederherstellen": "cap_restore",
    "kn_schliessen": "cap_close",
    "knopf_flaeche": "cap_surface",
    "ZK_NORMAL": "CAP_NORMAL",
    "ZK_UEBER": "CAP_HOVER",
    "ZK_DRUCK": "CAP_ACTIVE",
    "misch_kanal": "mix_channel",
    "deckung_ecke": "corner_coverage",
    "mal_mit": "blend_px",
    "stride": "stride",

    # ---------------------------------------------------- fui.textbuf
    "Textfeld": "TextBuf",
    "tf_new": "tb_new",
    "tf_init": "tb_init",
    "tf_ptr": "tb_ptr",
    "tf_len": "tb_len",
    "tf_cap": "tb_cap",
    "tf_zeiger": "tb_caret",
    "tf_anker": "tb_anchor",
    "tf_sel_a": "tb_sel_a",
    "tf_sel_b": "tb_sel_b",
    "tf_hat_sel": "tb_has_sel",
    "tf_komp_a": "tb_comp_a",
    "tf_komp_b": "tb_comp_b",
    "tf_hat_komp": "tb_has_comp",
    "tf_komp_setzen": "tb_comp_set",
    "tf_komp_ende": "tb_comp_end",
    "tf_text_setzen": "tb_set_text",
    "tf_leeren": "tb_clear",
    "tf_laenge_setzen": "tb_set_len",
    "tf_alles": "tb_select_all",
    "tf_setze": "tb_set_range",
    "tf_zeiger_setzen": "tb_set_caret",
    "tf_auswahl_weg": "tb_clear_sel",
    "tf_einfuegen_cp": "tb_insert_cp",
    "tf_einfuegen": "tb_insert",
    "tf_ersetzen": "tb_replace",
    "tf_loesche": "tb_delete",
    "tf_rueck": "tb_backspace",
    "tf_entf": "tb_delete_fwd",
    "tf_auswahl_loeschen": "tb_delete_sel",
    "tf_links": "tb_left",
    "tf_rechts": "tb_right",
    "tf_anfang": "tb_home",
    "tf_ende": "tb_end",
    "tf_vor": "tb_prev",
    "tf_nach": "tb_next",
    "tf_grenze": "tb_is_boundary",
    "tf_wort_um": "tb_word_at",
    "tf_wort_markieren": "tb_select_word",
    "tf_ist_wortzeichen": "tb_is_word_byte",
    "tf_kopieren": "tb_copy",
    "tf_ausschneiden": "tb_cut",
    "tf_x_von": "tb_x_of",
    "tf_zeiger_aus_x": "tb_caret_from_x",
    "tf_u16_von": "tb_u16_of",
    "tf_oktett_von_u16": "tb_byte_from_u16",
    "tf_u16_len": "tb_u16_len",
    "tf_cp_bei": "tb_cp_at",
    "tf_cp_kodieren": "tb_cp_encode",
    "tf_zieh": "tb_drag",
    "tf_set_zieh": "tb_set_drag",
    "tf_geheim": "tb_secret",
    "tf_set_geheim": "tb_set_secret",
    "zeiger": "caret",
    "anker": "anchor",
    "komp_a": "comp_a",
    "komp_b": "comp_b",
    "zieh": "drag",
    "geheim": "secret",
    "okt_bei": "byte_at",
    "okt": "byte_of",

    # ---------------------------------------------------- fui.editor
    "Edit": "Editor",
    "edit_neu": "editor_new",
    "edit_init": "editor_init",
    "edit_frei": "editor_free",
    "edit_feld": "editor_buf",
    "edit_setz_ablage": "editor_set_clipboard",
    "edit_setz_breite": "editor_set_width",
    "edit_rolle": "editor_scroll",
    "edit_taste": "editor_key",
    "edit_zeichen": "editor_char",
    "edit_maus_ab": "editor_mouse_down",
    "edit_maus_zieh": "editor_mouse_drag",
    "edit_maus_auf": "editor_mouse_up",
    "edit_marke_an": "editor_caret_on",
    "edit_marke_takt": "editor_caret_tick",
    "edit_marke_wecken": "editor_caret_wake",
    "edit_rueckgaengig": "editor_undo",
    "edit_wiederholen": "editor_redo",
    "edit_schritt_merken": "editor_push_undo",
    "edit_alles": "editor_select_all",
    "edit_text_setzen": "editor_set_text",
    "edit_leeren": "editor_clear",
    "edit_kopieren": "editor_copy",
    "edit_ausschneiden": "editor_cut",
    "edit_einfuegen": "editor_paste",
    "edit_sicht_setzen": "editor_scroll_to_caret",
    "edit_sicht": "editor_scroll_x",
    "wort_links": "word_left",
    "wort_rechts": "word_right",
    "T_LINKS": "KEY_LEFT",
    "T_RECHTS": "KEY_RIGHT",
    "T_POS1": "KEY_HOME",
    "T_ENDE": "KEY_END",
    "T_RUECK": "KEY_BACKSPACE",
    "T_ENTF": "KEY_DELETE",
    "T_EINGABE": "KEY_ENTER",
    "T_ESC": "KEY_ESC",
    "T_TAB": "KEY_TAB",
    "T_HOCH": "KEY_UP",
    "T_RUNTER": "KEY_DOWN",
    "M_UMSCH": "MOD_SHIFT",
    "M_STRG": "MOD_CTRL",
    "M_ALT": "MOD_ALT",
    "ANTWORT_NICHTS": "RES_NONE",
    "ANTWORT_GEAENDERT": "RES_CHANGED",
    "ANTWORT_BEWEGT": "RES_MOVED",
    "ANTWORT_FERTIG": "RES_COMMIT",
    "ANTWORT_ABBRUCH": "RES_CANCEL",
    "ANTWORT_WEITER": "RES_PASS",
    "UNDO_N": "UNDO_N",
    "UNDO_CAP": "UNDO_CAP",
    "nichts_lesen": "clip_read_none",
    "nichts_schreiben": "clip_write_none",
    "rolle": "scroll",
    "breite": "width",

    # ------------------------------------------------------ fui.icon
    "hak": "check",
    "punkt": "dot",
    "dreieck_ab": "triangle_down",
    "dreieck_auf": "triangle_up",
    "dreieck_links": "triangle_left",
    "dreieck_rechts": "triangle_right",
    "dreieck": "triangle",
    "kreuz": "cross",
    "minus": "dash",
    "griff_linien": "grip_lines",
    "strich": "stroke",
    "R_AB": "DIR_DOWN",
    "R_AUF": "DIR_UP",
    "R_LINKS": "DIR_LEFT",
    "R_RECHTS": "DIR_RIGHT",

    # ------------------------------------- gemeinsame lokale Namen
    "farbe": "color",
    "col": "col",
    "zustand": "state",
    "zustand_f": "state_col",
    "zustand_hat": "state_has",
    "eigen_f": "own_col",
    "eigen_hat": "own_has",
    "wid": "def",
    "senkrecht": "vertical",
    "halten": "extend",
    "sichtbar": "visible",
    "ganz": "total",
    "gross": "size",
    "wie": "how",
    "raum": "space",
    "wunsch": "pref",
    "innen": "inner",
    "spalten": "cols",
    "zeilen": "rows",
    "sp": "col_i",
    "links": "left",
    "oben": "top",
    "rechts": "right",
    "unten": "bottom",
    "staerke": "width_px",
    "aus": "align",
    "base": "base_y",
    "vorgabe": "fallback",
}


def build_pattern(mapping):
    keys = sorted(mapping.keys(), key=len, reverse=True)
    return re.compile(r"\b(" + "|".join(re.escape(k) for k in keys) + r")\b")


def apply_to_text(text, mapping, pattern):
    return pattern.sub(lambda m: mapping[m.group(1)], text)


def main():
    root = "/root/firn-fui"
    dry = "--dry" in sys.argv

    # 1. Die Modulpfade: `import fui.stil` -> `import fui.style`,
    #    und jede Qualifizierung `stil.` -> `style.`.
    mod_pat = re.compile(r"\bfui\.(" + "|".join(MODULE.keys()) + r")\b")
    qual_pat = re.compile(r"\b(" + "|".join(MODULE.keys()) + r")\.")

    name_pat = build_pattern(NAMES)

    files = []
    for base in ("lib/fui", "tools/fui"):
        d = os.path.join(root, base)
        for fn in sorted(os.listdir(d)):
            if fn.endswith(".fi"):
                files.append(os.path.join(d, fn))

    changed = 0
    for path in files:
        with open(path, encoding="utf-8") as fh:
            src = fh.read()
        out = src
        # Reihenfolge: erst die Qualifizierer (stil. -> style.), dann
        # die Bezeichner. Sonst wuerde `stil_setz_hg` das `stil`
        # darin anfassen -- \b schuetzt davor, aber die Reihenfolge
        # macht es zusaetzlich eindeutig.
        out = mod_pat.sub(lambda m: "fui." + MODULE[m.group(1)], out)
        out = qual_pat.sub(lambda m: MODULE[m.group(1)] + ".", out)
        out = apply_to_text(out, NAMES, name_pat)
        if out != src:
            changed += 1
            if not dry:
                with open(path, "w", encoding="utf-8") as fh:
                    fh.write(out)
            print("  geaendert: " + os.path.relpath(path, root))

    print("\n%d Dateien geaendert." % changed)

    # 2. Die Dateien umbenennen.
    if not dry:
        for alt, neu in MODULE.items():
            a = os.path.join(root, "lib/fui", alt + ".fi")
            b = os.path.join(root, "lib/fui", neu + ".fi")
            if os.path.exists(a):
                os.rename(a, b)
                print("  lib/fui/%s.fi -> %s.fi" % (alt, neu))
        for fn in sorted(os.listdir(os.path.join(root, "tools/fui"))):
            if not fn.endswith("_main.fi"):
                continue
            stem = fn[:-8]
            if stem in MODULE:
                a = os.path.join(root, "tools/fui", fn)
                b = os.path.join(root, "tools/fui",
                                 MODULE[stem] + "_main.fi")
                os.rename(a, b)
                print("  tools/fui/%s -> %s_main.fi" % (fn, MODULE[stem]))


main()
