import * as m from "$lib/paraglide/messages.js";

export function nativeMenuLabels(locale: "sv" | "en") {
  const options = { locale };
  return {
    file: m.native_menu_file({}, options),
    edit: m.native_menu_edit({}, options),
    view: m.native_menu_view({}, options),
    window: m.native_menu_window({}, options),
    help: m.native_menu_help({}, options),
    about: m.native_menu_about({}, options),
    services: m.native_menu_services({}, options),
    hide: m.native_menu_hide({}, options),
    hideOthers: m.native_menu_hideOthers({}, options),
    quit: m.native_menu_quit({}, options),
    close: m.native_menu_close({}, options),
    undo: m.native_menu_undo({}, options),
    redo: m.native_menu_redo({}, options),
    cut: m.native_menu_cut({}, options),
    copy: m.native_menu_copy({}, options),
    paste: m.native_menu_paste({}, options),
    selectAll: m.native_menu_selectAll({}, options),
    fullscreen: m.native_menu_fullscreen({}, options),
    minimize: m.native_menu_minimize({}, options),
    maximize: m.native_menu_maximize({}, options),
    zoomIn: m.zoom_in({}, options),
    zoomOut: m.zoom_out({}, options),
    actualSize: m.zoom_actual_size({}, options)
  };
}
