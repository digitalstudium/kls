import curses
import time
import threading

def inactive_function():
    # This function will be executed while the screen is inactive
    print("Screen is inactive, doing some work...")
    # Do some work here, e.g., update a database, send a notification, etc.
    time.sleep(5)  # simulate some work being done
    print("Work done!")

def main(screen):
    screen.nodelay(True)  # don't wait for input
    threading.Thread(target=inactive_function).start()  # start the inactive function in a separate thread
    while True:
        screen.clear()
        screen.addstr(0, 0, f"{time.time()}")
        time.sleep(1)
        screen.refresh()
        if screen.getch() != -1:  # if there's input, exit
            break

curses.wrapper(main)
