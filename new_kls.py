#!/usr/bin/env python3
import curses
import subprocess

# Constants
SELECTED_WITHOUT_SEARCH = 1
SELECTED_WITH_SEARCH = 2
NOT_SELECTED_WITHOUT_SEARCH = 3
NOT_SELECTED_WITH_SEARCH = 4

class Menu:
    def __init__(self, name, rows, begin_x, width, state):
        self.state = state
        self.name = name
        self.rows = rows
        self.selected_row = 0
        self.begin_x = begin_x
        self.win = curses.newwin(curses.LINES, width, 0, begin_x)
        self.rows_number = curses.LINES - 10
        self.filter = ""

    def filter_rows(self):
        return list(filter(lambda x: self.filter in x, self.rows))

    def get_selected_row(self):
        filtered_rows = self.filter_rows()
        if filtered_rows:
            return min(self.selected_row, len(filtered_rows) - 1)
        return 0

    def draw_header(self):
        header_attr = curses.A_BOLD | curses.color_pair(1) if self.state in [1, 2] else curses.A_NORMAL
        self.win.addstr(1, 2, self.name, header_attr)
        self.win.refresh()

    def draw_rows(self):
        filtered_rows = self.filter_rows()
        if not filtered_rows:
            return
        first_row_index = max(0, self.selected_row - self.rows_number + 1)
        for index, row in enumerate(filtered_rows[first_row_index:]):
            attr = curses.color_pair(2)
            if index + first_row_index == self.get_selected_row():
                attr |= curses.color_pair(1) | curses.A_BOLD
            self.win.addstr(index + 3, 2, row, attr)
        self.win.box()
        self.win.refresh()

    def draw_search_box(self):
        search_attr = curses.A_BOLD if self.state in [SELECTED_WITH_SEARCH, NOT_SELECTED_WITH_SEARCH] else curses.A_NORMAL
        content = f"/{self.filter}" if self.state in [SELECTED_WITH_SEARCH, NOT_SELECTED_WITH_SEARCH] else "Press / for search"
        self.win.addstr(curses.LINES - 2, 2, content, search_attr)
        self.win.clrtoeol()
        self.win.box()
        self.win.refresh()

    def draw_menu(self):
        self.win.clear()
        self.draw_header()
        self.draw_rows()
        self.draw_search_box()


def run_command(namespace, api_resource, resource, key_pressed):
    commands = {
        "KEY_F(1)": f'kubectl -n {namespace} get {api_resource} {resource} -o yaml | batcat -l yaml --paging always --style numbers',
        "KEY_F(2)": f'kubectl -n {namespace} describe {api_resource} {resource} | batcat -l yaml --paging always --style numbers',
        "KEY_F(3)": f'kubectl edit {api_resource} -n {namespace} {resource}',
        "KEY_F(4)": f'kubectl -n {namespace} logs {resource} | batcat -l log --paging always --style numbers'
    }
    command = commands.get(key_pressed)
    if command:
        curses.def_shell_mode()
        curses.endwin()
        subprocess.call(command, shell=True)
        curses.reset_shell_mode()


def main(stdscr):
    curses.curs_set(0)
    curses.noecho()
    curses.start_color()
    curses.use_default_colors()

    menus = []
    # Initialize menus
    for i, (name, rows, begin_x, width, state) in enumerate([
        ("Namespaces", subprocess.check_output("kubectl get ns --no-headers | awk '{print $1}'", shell=True).decode("utf-8").split("\n"), 0, curses.COLS // 10 * 2, 1),
        ("API resources", subprocess.check_output("kubectl api-resources --no-headers --verbs get | awk '{print $1}'", shell=True).decode("utf-8").split("\n"), curses.COLS // 10 * 2, curses.COLS // 10 * 3, 3),
        ("Resources", subprocess.check_output("kubectl get pods --no-headers -n default | awk '{print $1}'", shell=True).decode("utf-8").split("\n"), curses.COLS // 10 * 5, curses.COLS - curses.COLS // 10 * 5, 3)
    ]):
        menus.append(Menu(name, rows, begin_x, width, state))

    while True:
        for menu in menus:
            menu.draw_menu()
            key_pressed = stdscr.getkey()
            if key_pressed in ("KEY_RIGHT", "KEY_LEFT", "\t"):
                next_menu = menus[(menus.index(menu) + 1) % len(menus)]
                menu.state = NOT_SELECTED_WITH_SEARCH if menu.filter else NOT_SELECTED_WITHOUT_SEARCH
                next_menu.state = SELECTED_WITH_SEARCH if next_menu.filter else SELECTED_WITHOUT_SEARCH
            elif key_pressed in ("KEY_UP", "KEY_DOWN"):
                menu.selected_row += 1 if key_pressed == "KEY_DOWN" else -1
                if menu.state == SELECTED_WITH_SEARCH:
                    menu.selected_row = max(0, menu.selected_row)
            elif key_pressed == "KEY_F(4)":
                namespace = menus[0].filter_rows()[menus[0].get_selected_row()]
                api_resource = menus[1].filter_rows()[menus[1].get_selected_row()]
                resource = menus[2].filter_rows()[menus[2].get_selected_row()]
                run_command(namespace, api_resource, resource, key_pressed)

if __name__ == "__main__":
    curses.wrapper(main)
