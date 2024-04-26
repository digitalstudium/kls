#!/usr/bin/env python3
import subprocess, curses, time

KEY_BINDINGS = {  # can be extended
    "1": 'kubectl -n {namespace} get {api_resource} {resource} -o yaml | batcat -l yaml --paging always --style numbers',
    "\n": 'kubectl -n {namespace} get {api_resource} {resource} -o yaml | batcat -l yaml --paging always --style numbers',  # Enter key
    "2": 'kubectl -n {namespace} describe {api_resource} {resource} | batcat -l yaml --paging always --style numbers',
    "3": 'kubectl -n {namespace} edit {api_resource} {resource}',
    "4": 'kubectl -n {namespace} logs {resource} | batcat -l log --paging always --style numbers',
    "5": 'kubectl -n {namespace} exec -it {resource} sh',
    "6": 'kubectl -n {namespace} debug {resource} -it --image=nicolaka/netshoot',
    "KEY_DC": 'kubectl -n {namespace} delete {api_resource} {resource}'  # KEY_DC is the delete key
}
# which api resources are on the top of menu?
TOP_API_RESOURCES = ["pods", "services", "configmaps", "secrets", "persistentvolumeclaims", "ingresses", "nodes",
                     "deployments", "statefulsets", "daemonsets",  "storageclasses", "all"]

HELP_TEXT = "Esc: exit filter mode or exit kls, 1/Enter: get yaml, 2: describe, 3: edit, 4: logs, 5: exec, 6: debug, arrows/TAB: navigation"

MOUSE_ENABLED = True

SCREEN = curses.initscr()  # screen initialization, needed for ROWS_HEIGHT working
HEADER_HEIGHT = 4  # in rows
FOOTER_HEIGHT = 3
ROWS_HEIGHT = curses.LINES - HEADER_HEIGHT - FOOTER_HEIGHT - 3   # maximum number of visible rows indices
WIDTH = curses.COLS


class Menu:
    def __init__(self, title: str, rows: list, begin_x: int, width: int):
        self.title = title
        self.rows = rows  # all rows
        self.filter = ""  # filter for rows
        self.filtered_rows = lambda: [x for x in self.rows if self.filter in x]  # filtered rows
        self.filtered_row_index = 0  # index of the selected filtered row
        # __start_index - starting from which row we will select rows from filtered_rows()? Usually from the first row,
        # but if the size of filtered_rows is greater than HEIGHT and filtered_row_index exceeds the menu HEIGHT,
        # we shift __start_index to the right by filtered_row_index - HEIGHT. This way we implement menu scrolling
        self.__start_index = lambda: 0 if self.filtered_row_index < ROWS_HEIGHT else self.filtered_row_index - ROWS_HEIGHT + 1
        self.visible_rows = lambda: self.filtered_rows()[self.__start_index():][:ROWS_HEIGHT]  # visible rows
        self.__visible_row_index = lambda: self.filtered_row_index - self.__start_index()  # index of the selected visible row
        # selected row from visible rows
        self.selected_row = lambda: self.visible_rows()[self.__visible_row_index()] if self.visible_rows() else None
        self.width = width
        self.begin_x = begin_x
        self.win = curses.newwin(curses.LINES - FOOTER_HEIGHT, width, 0, begin_x)


def draw_row(window: curses.window, text: str, y: int, x: int, selected: bool = False):
    window.addstr(y, x, text, curses.A_REVERSE | curses.A_BOLD if selected else curses.A_NORMAL)
    window.clrtoeol()
    window.refresh()


def draw_rows(menu: Menu):
    for index, row in enumerate(menu.visible_rows()):
        draw_row(menu.win, row, index + HEADER_HEIGHT, 2, selected=True if row == menu.selected_row() else False)


def draw_menu(menu: Menu):
    menu.win.clear()  # clear menu window
    draw_row(menu.win, menu.title, 1, 2, selected=True if menu == SELECTED_MENU else False)  # draw title
    draw_rows(menu)  # draw menu rows
    draw_row(menu.win, f"/{menu.filter}" if menu.filter else "", curses.LINES - FOOTER_HEIGHT - 2, 2)  # draw filter row


def refresh_third_menu():
    MENUS[2].rows = []
    if api_resource() and namespace():
        MENUS[2].rows = kubectl(f"-n {namespace()} get {api_resource()} --no-headers --ignore-not-found")
    draw_menu(MENUS[2])


def run_command(key: str, api_resource: str, resource: str):
    if key in ("4", "5", "6"):
        if api_resource not in ["pods", "all"] or (api_resource == "all" and not resource.startswith("pod/")):
            return
    curses.def_prog_mode()  # save the previous terminal state
    curses.endwin()  # without this, there are problems after exiting vim
    command = KEY_BINDINGS[key].format(namespace=namespace(), api_resource=api_resource, resource=resource)
    if api_resource == "all":
        command = command.replace(" all", "")
    subprocess.call(command, shell=True)
    curses.reset_prog_mode()  # restore the previous terminal state
    SCREEN.refresh()
    curses.mousemask(curses.REPORT_MOUSE_POSITION)  # mouse tracking
    print('\033[?1003h') # enable mouse tracking with the XTERM API. That's the magic


def handle_filter_state(key: str, menu: Menu):
    if key == "\x1b":
        menu.filter = ""  # Escape key exits filter mode
    elif key in ["KEY_BACKSPACE", "\x08"]:
        menu.filter = menu.filter[:-1]  # Backspace key deletes a character (\x08 is also Backspace)
    elif key.isalpha() or key == "-":
        menu.filter += key.lower()
    else:
        return
    menu.filtered_row_index = 0
    draw_menu(menu)
    if menu != MENUS[2]:
        MENUS[2].filtered_row_index = 0  # reset the selected row index of third menu before redrawing


def handle_mouse(mouse_info: tuple, menu: Menu):
    row_number = mouse_info[2] - HEADER_HEIGHT
    column_number = mouse_info[1]
    next_menu = None
    if column_number > (menu.begin_x + menu.width):
        next_menu = MENUS[(MENUS.index(menu) + 1) % 3]
        if column_number > (next_menu.begin_x + next_menu.width):
            next_menu = MENUS[(MENUS.index(next_menu) + 1) % 3]
        globals().update(SELECTED_MENU=next_menu)
    elif column_number < menu.begin_x:
        next_menu = MENUS[(MENUS.index(menu) - 1) % 3]
        if column_number < next_menu.begin_x:
            next_menu = MENUS[(MENUS.index(next_menu) - 1) % 3]
        globals().update(SELECTED_MENU=next_menu)
    if next_menu:
        draw_row(menu.win, menu.title, 1, 2, selected=False)  # remove selection from the current menu title
        draw_row(next_menu.win, next_menu.title, 1, 2, selected=True)  # and select the new menu title
        menu = next_menu
    char_int = menu.win.inch(mouse_info[2], column_number - menu.begin_x - 1) # get char from current mouse position
    char_str = chr(char_int & 0xFF)
    if not char_str or ord(char_str) > 127 or ' ' in char_str:
        return
    if 0 <= row_number < len(menu.visible_rows()):
        menu.filtered_row_index = row_number
        draw_rows(menu)  # this will change selected row in menu
        if menu != MENUS[2]:
            MENUS[2].filtered_row_index = 0  # reset the selected row index of third menu before redrawing


def catch_input(menu: Menu):
    while True:  # refresh third menu until key pressed
        try:
            key = SCREEN.getkey()
            break
        except curses.error:
            refresh_third_menu()
            time.sleep(0.1)
    if key in ["\t", "KEY_RIGHT", "KEY_BTAB", "KEY_LEFT"]:
        increment = {"KEY_RIGHT": 1, "\t": 1, "KEY_LEFT": -1, "KEY_BTAB": -1}[key]
        next_menu = MENUS[(MENUS.index(menu) + increment) % 3]
        draw_row(menu.win, menu.title, 1, 2, selected=False)  # remove selection from the current menu title
        draw_row(next_menu.win, next_menu.title, 1, 2, selected=True)  # and select the new menu title
        globals().update(SELECTED_MENU=next_menu)
    elif key in ["KEY_UP", "KEY_DOWN"] and len(menu.visible_rows()) > 1:
        increment = {"KEY_DOWN": 1, "KEY_UP": -1}[key]
        menu.filtered_row_index = (menu.filtered_row_index + increment) % len(menu.filtered_rows())
        draw_rows(menu)  # this will change selected row in menu
        if menu != MENUS[2]:
            MENUS[2].filtered_row_index = 0  # reset the selected row index of third menu before redrawing
    elif key == "KEY_MOUSE" and MOUSE_ENABLED:
        mouse_info = curses.getmouse()
        handle_mouse(mouse_info, menu)
    elif key in KEY_BINDINGS.keys() and MENUS[2].selected_row():
        run_command(key, api_resource(), resource())
    elif key == "\x1b" and not menu.filter:
        globals().update(SELECTED_MENU=None)  # exit
    else:
        handle_filter_state(key, menu)


def kubectl(command: str) -> list:
    return subprocess.check_output(f"kubectl {command}", shell=True).decode().strip().split("\n")


api_resources_kubectl = [x.split()[0] for x in kubectl("api-resources --no-headers --verbs=get")]
api_resources = list(dict.fromkeys(TOP_API_RESOURCES + api_resources_kubectl))  # so top api resources are at the top
width_unit = WIDTH // 8
MENUS = [Menu("Namespaces", kubectl("get ns --no-headers -o custom-columns=NAME:.metadata.name"), 0, width_unit),
         Menu("API resources", api_resources, width_unit, width_unit * 2),
         Menu("Resources", [], width_unit * 3, WIDTH - width_unit * 3)]
SELECTED_MENU = MENUS[0]
namespace = MENUS[0].selected_row  # method alias
api_resource = MENUS[1].selected_row
resource = lambda: MENUS[2].selected_row().split()[0]


def main(screen):
    SCREEN.refresh()  # I don't know why this is needed but it doesn't work without it
    SCREEN.nodelay(True)  # don't block while waiting for input
    SCREEN.keypad(True)  # needed for arrow keys
    curses.set_escdelay(1)  # reduce Escape delay to 1 ms (curses can't set it to 0)
    curses.curs_set(0)  # make the cursor invisible
    curses.use_default_colors()  # don't change the terminal color
    curses.noecho()  # don't output characters at the top
    curses.mousemask(curses.REPORT_MOUSE_POSITION)  # mouse tracking
    print('\033[?1003h') # enable mouse tracking with the XTERM API. That's the magic
    for menu in MENUS:  # draw the main windows
        draw_menu(menu)
    draw_row(curses.newwin(3, curses.COLS, curses.LINES - FOOTER_HEIGHT, 0), HELP_TEXT, 1, 2)  # and the help window
    while SELECTED_MENU:
        catch_input(SELECTED_MENU)  # if a menu is selected, catch user input


curses.wrapper(main)