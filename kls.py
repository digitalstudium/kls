#!/usr/bin/env python3
import curses
import subprocess

# инициализация экрана
screen = curses.initscr()
screen.refresh()  # не знаю зачем это нужно но без этого не работает
curses.set_escdelay(1) # в curses зачем-то сделали задержку на срабатывание Escape, уменьшаем её до 1 милисекунды (до 0 нельзя)
screen.keypad(True)  # нужно для работы с клавишами F1-F4
curses.curs_set(0)  # делаем курсор невидимым
curses.noecho()  # не выводим символы вверху
curses.start_color()  # инициализируем цвета
if curses.has_colors():
    curses.init_pair(1, curses.COLOR_WHITE, curses.COLOR_BLUE)  # белый на голубом - для заголовка
    curses.init_pair(2, curses.COLOR_WHITE, curses.COLOR_BLACK)  # белый на чёрном - для остальных строк

# состояния меню
SELECTED_WITHOUT_SEARCH = 1  # выбрано и поиск выключен
SELECTED_WITH_SEARCH = 2  # выбрано и поиск включен
NOT_SELECTED_WITHOUT_SEARCH = 3  # не выбрано и поиск выключен
NOT_SELECTED_WITH_SEARCH = 4  # не выбрано и поиск включен


class Menu:
    def __init__(self, name, rows, begin_x, width, state):
        self.state = state  # состояние меню
        self.name = name  # заголовок меню
        self.rows = rows  # строки меню
        self.selected_row = 0  # выбранная строка меню
        self.begin_x = begin_x  # где начинается меню по х?
        self.win = curses.newwin(curses.LINES, width, 0, begin_x)  # окно с высотой во весь экран, шириной width, и началом по х в точке begin_x
        self.rows_number = curses.LINES - 10  # максимальное число видимых строк меню, начиная с 0
        self.filter = ""  # фильтр строк меню


# инициализируем меню
namespaces = [ns for ns in subprocess.check_output("kubectl get ns --no-headers | awk '{print $1}'", shell=True).decode("utf-8").split("\n") if ns]
menu1 = Menu("Namespaces", namespaces, 0, curses.COLS // 5, SELECTED_WITHOUT_SEARCH)

api_resources_top = ["pods", "services", "deployments", "statefulsets", "daemonsets", "ingresses", "configmaps", "secrets", "persistentvolumes", "persistentvolumeclaims", "nodes", "storageclasses"]
api_resources_kubectl = [i for i in subprocess.check_output("kubectl api-resources --no-headers --verbs get | awk '{print $1}'", shell=True).decode("utf-8").split("\n") if i]
api_resources = api_resources_top + sorted(list(set(api_resources_kubectl) - set(api_resources_top)))
menu2 = Menu("API resources", api_resources, 0 + curses.COLS // 5, curses.COLS // 5 * 2, NOT_SELECTED_WITHOUT_SEARCH)

pods = [p for p in subprocess.check_output(f"kubectl get pods --no-headers -n {namespaces[0]} | awk '{{print $1}}'", shell=True).decode("utf-8").split("\n") if p]
menu3 = Menu("Resources", pods, 0 + curses.COLS // 5 * 3, curses.COLS // 5 * 2, NOT_SELECTED_WITHOUT_SEARCH)

menus = [menu1, menu2, menu3]


def update_menu3_object():
    menu1_filtered_rows = list(filter(lambda x: (menu1.filter in x), menu1.rows))  # фильтруем строки
    menu2_filtered_rows = list(filter(lambda x: (menu2.filter in x), menu2.rows))  # фильтруем строки
    if not menu1_filtered_rows or not menu2_filtered_rows:
        menu3.rows = ["No resources matched criteria.", ]
    else:
        namespace = menu1_filtered_rows[menu1.selected_row]
        api_resource = menu2_filtered_rows[menu2.selected_row]
        menu3.rows = [r for r in subprocess.check_output(f"kubectl get {api_resource} --no-headers -n {namespace} | awk '{{print $1}}'", shell=True).decode("utf-8").split("\n") if r]
        if not menu3.rows: menu3.rows = [f"No resources found in {namespace} namespace.", ]
    menu3.selected_row = 0


def draw_header(menu):
    header_attr = curses.A_BOLD | curses.color_pair(1) if menu.state in [1, 2] else curses.A_NORMAL
    menu.win.addstr(1, 2, menu.name, header_attr)
    menu.win.refresh()


def draw_rows(menu):
    filtered_rows = list(filter(lambda x: (menu.filter in x), menu.rows))  # какие строки сейчас в меню, учитывая фильтр?
    if not filtered_rows: return  # если строк нет, рисовать их не нужно
    # ограничиваем число отфильтрованных строк высотой окна + выбираем, от какой cтроки меню будет начинаться меню
    first_row_index = 0 if menu.selected_row < menu.rows_number else menu.selected_row - menu.rows_number + 1
    last_row_index = first_row_index + menu.rows_number
    filtered_rows = filtered_rows[first_row_index:last_row_index]
    selected_row_index = menu.selected_row - first_row_index  # индекс выбранной строки в отфильтрованных строках
#    if menu2.selected_row != 0:  # debug
#        raise ValueError(f"{len(filtered_rows)} {selected_row_index} {first_row_index} {last_row_index} {menu.rows_number}")
    for index, row in enumerate(filtered_rows):  # рисуем то, что отфильтровали
        menu.win.addstr(index + 3, 2, row, curses.color_pair(2))
    menu.win.addstr(selected_row_index + 3, 2, filtered_rows[selected_row_index], curses.color_pair(1) | curses.A_BOLD)  # выделяем выбранную строку
    menu.win.box()
    menu.win.refresh()


def draw_search_box(menu):
    search_attr = curses.A_BOLD if menu.state in [SELECTED_WITH_SEARCH, NOT_SELECTED_WITH_SEARCH] else curses.A_NORMAL
    content = f"/{menu.filter}" if menu.state in [SELECTED_WITH_SEARCH, NOT_SELECTED_WITH_SEARCH] else "Press / for search"
    menu.win.addstr(curses.LINES - 2, 2, content, search_attr)  # рисуем контент
    menu.win.clrtoeol()  # очищаем остальную часть строки
    menu.win.box()  # рисуем рамку
    menu.win.refresh()  # обновляем окно


def draw_menu(menu):
    menu.win.clear()  # очищаем окно меню
    draw_header(menu)  # рисуем заголовок
    draw_rows(menu)  # рисуем строки меню
    draw_search_box(menu)  # рисуем строку поиска


def draw_window():
    for menu in menus:
        draw_menu(menu)


def run_command(key_pressed):
    menu3_filtered_rows = list(filter(lambda x: (menu3.filter in x), menu3.rows))  # фильтруем строки меню 3
    if not menu3_filtered_rows or menu3_filtered_rows[0].startswith("No resources"): return
    menu1_filtered_rows = list(filter(lambda x: (menu1.filter in x), menu1.rows))  # фильтруем строки
    menu2_filtered_rows = list(filter(lambda x: (menu2.filter in x), menu2.rows))  # фильтруем строки
    namespace = menu1_filtered_rows[menu1.selected_row]
    api_resource = menu2_filtered_rows[menu2.selected_row]
    resource = menu3.rows[menu3.selected_row]
    match key_pressed:
        case "KEY_F(1)":
            command = f'kubectl -n {namespace} get {api_resource} {resource} -o yaml | batcat -l yaml --paging always --style numbers'
        case "KEY_F(2)":  # describe
            command = f'kubectl -n {namespace} describe {api_resource} {resource} | batcat -l yaml --paging always --style numbers'
        case "KEY_F(3)":  # edit
            command = f'kubectl edit {api_resource} -n {namespace} {resource}'
        case "KEY_F(4)":
            if api_resource != "pods": return
            command = f'kubectl -n {namespace} logs {resource} | batcat -l log --paging always --style numbers'
    curses.def_shell_mode()
    curses.endwin()
    subprocess.call(command, shell=True)
    curses.reset_shell_mode()
    draw_window()


def navigate_horizontally(direction, menu):
    increment = {"right": 1, "left": -1}
    menu_index = {menu1: 0, menu2: 1, menu3: 2}  # порядковые номера меню
    next_menu = menus[(menu_index[menu] + increment[direction]) % 3]
    menu.state = NOT_SELECTED_WITH_SEARCH if menu.filter else NOT_SELECTED_WITHOUT_SEARCH
    next_menu.state = SELECTED_WITH_SEARCH if next_menu.filter else SELECTED_WITHOUT_SEARCH
    draw_header(menu)  # убираем выделение с заголовка текущего меню
    draw_header(next_menu)  # выделяем заголовок следующего/предыдущего меню


def navigate_vertically(direction, menu):
    filtered_rows = list(
        filter(lambda x: (menu.filter in x), menu.rows))  # какие строки сейчас в меню, учитывая фильтр?
    if not filtered_rows or len(filtered_rows) == 1: return  # если строк нет или строка одна, навигация не нужна
    increment = {"down": 1, "up": -1}
    menu.selected_row = (menu.selected_row + increment[direction]) % len(
        filtered_rows)  # выбираем строку учитывая сколько строк в меню
    draw_menu(menu)  # перерисовываем меню


def handle_selected_with_search_state(key_pressed, menu):
    if key_pressed == "\x1b":  # Escape key exits search mode
        menu.filter = ""
        menu.selected_row = 0
        menu.state = SELECTED_WITHOUT_SEARCH
        draw_menu(menu)
    elif key_pressed == "KEY_BACKSPACE":
        menu.state = SELECTED_WITHOUT_SEARCH if not menu.filter else menu.state
        menu.filter = menu.filter[:-1] if menu.filter else ""
        draw_menu(menu)
    elif key_pressed.isalpha() or key_pressed == "-":
        menu.filter += key_pressed
        menu.selected_row = 0
        draw_menu(menu)


def handle_selected_without_search_state(key_pressed, menu):
    if key_pressed == "/":
        menu.state = SELECTED_WITH_SEARCH
        draw_search_box(menu)
    elif key_pressed == "q":
        return "interrupt"


def catch_input(menu):
    key_pressed = screen.getkey()
    if key_pressed == '\t' or key_pressed == "KEY_RIGHT": navigate_horizontally("right", menu)
    elif key_pressed == "KEY_BTAB" or key_pressed == "KEY_LEFT": navigate_horizontally("left", menu)
    elif key_pressed == "KEY_DOWN": navigate_vertically("down", menu)
    elif key_pressed == "KEY_UP": navigate_vertically("up", menu)
    elif key_pressed in ["KEY_F(1)", "KEY_F(2)", "KEY_F(3)", "KEY_F(4)"]: run_command(key_pressed)
    elif menu.state == SELECTED_WITH_SEARCH: handle_selected_with_search_state(key_pressed, menu)
    elif menu.state == SELECTED_WITHOUT_SEARCH:
        result = handle_selected_without_search_state(key_pressed, menu)
        if result:
            return result
    if menu != menu3 and key_pressed not in ["KEY_RIGHT", "KEY_LEFT", "\t", "KEY_BTAB", "/"]:
        update_menu3_object()
        draw_menu(menu3)


def main():
    draw_window()  # рисуем начальный экран
    state = "running"
    while state != "interrupt":
         for menu in menus:
             if menu.state in [1, 2]: state = catch_input(menu)


main()
curses.echo()
curses.endwin()
subprocess.call(["reset"])  # Потому что терминал не работает без этого после выхода из kls
