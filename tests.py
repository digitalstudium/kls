import unittest
from unittest.mock import patch
import os

os.system("ln -s kls kls.py")

import kls

os.unlink('kls.py')


class TestCircularList(unittest.TestCase):
    def setUp(self):
        self.circular_list = kls.CircularList(['kube-system', 'default', 'kube-public'])

    def test_forward(self):
        self.circular_list.shift(1)
        self.assertEqual(self.circular_list.index, 1)

    def test_backward(self):
        self.circular_list.shift(-1)
        self.assertEqual(self.circular_list.index, 2)  # Since it's circular, it goes to the end

    def tearDown(self):
        kls.curses.endwin()


class TestScriptFunctions(unittest.TestCase):
    @patch('kls.subprocess.check_output')
    def test_kubectl(self, mock_check_output):
        mock_check_output.return_value = b'pod1\npod2\npod3'
        result = kls.kubectl('get pods')
        self.assertEqual(result, ['pod1', 'pod2', 'pod3'])

    def tearDown(self):
        kls.curses.endwin()


class TestMenu(unittest.TestCase):
    def setUp(self):
        self.rows = ['kube-system', 'default', 'kube-public']
        self.menu = kls.Menu('Test', self.rows, 0, 10, 2)
        self.second_menu = kls.Menu("Test Menu 2", ["pods", "services", "secrets"], 0, 10, 2)
        self.third_menu = kls.Menu("Test Menu 3", ['pod1', 'pod2', 'pod3'], 0, 10, 2)
        kls.menus = [self.menu, self.second_menu, self.third_menu]  # Add the menu to the list of menus
        os.system("ln -s kls kls.py")

    def test_menu(self):
        self.assertEqual(self.menu.title, 'Test')
        self.assertEqual(self.menu.filtered_rows.elements, self.rows)
        self.assertEqual(self.menu.visible_rows(), ['kube-system', 'default'])
        self.assertEqual(self.menu.selected_row(), 'kube-system')

    def test_filter_rows_with_filter(self):
        # Apply a filter and test
        self.menu.filter = 'kube-system'
        self.menu.filtered_rows = kls.CircularList([x for x in self.menu.rows if self.menu.filter in x])
        self.assertEqual(self.menu.filtered_rows.elements, ['kube-system'])

    def test_filter_rows_with_nonexistent_filter(self):
        # Apply a filter that matches no rows
        self.menu.filter = 'nonexistent'
        self.menu.filtered_rows = kls.CircularList([x for x in self.menu.rows if self.menu.filter in x])
        self.assertEqual(self.menu.filtered_rows.elements, [])

    def test_vertical_navigation(self):
        kls.selected_menu = self.menu
        # Test moving down one row
        kls.handle_vertical_navigation("KEY_DOWN", self.menu)
        self.assertEqual(self.menu.visible_row_index, 0)

        # Test moving up one row
        kls.handle_vertical_navigation("KEY_UP", self.menu)
        self.assertEqual(self.menu.visible_row_index, 0)

        # Test moving to the next page
        kls.handle_vertical_navigation("KEY_NPAGE", self.menu)
        self.assertEqual(self.menu.visible_row_index, 0)

        # Test moving to the previous page
        kls.handle_vertical_navigation("KEY_PPAGE", self.menu)
        self.assertEqual(self.menu.visible_row_index, 0)

        # Test moving to the first row
        kls.handle_vertical_navigation("KEY_HOME", self.menu)
        self.assertEqual(self.menu.visible_row_index, 0)

        # Test moving to the last row
        kls.handle_vertical_navigation("KEY_END", self.menu)
        self.assertEqual(self.menu.visible_row_index, -1)

    @patch('kls.subprocess.call')
    @patch('kls.curses.reset_prog_mode')
    @patch('kls.curses.def_prog_mode')
    def test_handle_key_bindings(self, mock_def_prog_mode, mock_reset_prog_mode, mock_subprocess_call):
        namespace = self.menu.selected_row()
        api_resource = self.second_menu.selected_row()
        resource = self.third_menu.selected_row()

        key = "1"  # Assuming you want to test the case where key is '1'
        expected_command = kls.KEY_BINDINGS[key].format(namespace=namespace, api_resource=api_resource, resource=resource)

        kls.handle_key_bindings(key, namespace, api_resource, resource)

        mock_def_prog_mode.assert_called_once()
        mock_reset_prog_mode.assert_called_once()
        mock_subprocess_call.assert_called_once_with(expected_command, shell=True)

    @patch('kls.curses.def_prog_mode')
    def test_handle_key_bindings_empty_resource(self, mock_def_prog_mode):
        namespace = self.menu.selected_row()
        api_resource = self.second_menu.selected_row()
        resource = None

        key = "1"  # Assuming you want to test the case where key is '1'

        kls.handle_key_bindings(key, namespace, api_resource, resource)
        mock_def_prog_mode.assert_not_called()

    @patch('kls.curses.def_prog_mode')
    def test_handle_key_bindings_services(self, mock_def_prog_mode):
        namespace = self.menu.selected_row()
        api_resource = "services"
        resource = self.third_menu.selected_row()

        key = "4"  # 4 must not be called for services

        kls.handle_key_bindings(key, namespace, api_resource, resource)
        mock_def_prog_mode.assert_not_called()

    def tearDown(self):
        os.unlink('kls.py')  # Remove the symlink after each test
        kls.curses.endwin()
        print('\033[?1003l')  # Disable mouse tracking with the XTERM API


if __name__ == '__main__':
    unittest.main()