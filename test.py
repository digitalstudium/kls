import time
import curses


def draw(canvas):
    while True:
        key = canvas.getkey()
        if key == ("\x1b"):
            print(f"Вы ввели Escape!")
        print(f"Вы ввели {key}")
    
  
if __name__ == '__main__':
    curses.update_lines_cols()
    curses.wrapper(draw)

