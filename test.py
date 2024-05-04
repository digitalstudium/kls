import unittest
from unittest.mock import patch
import os

os.system("ln -s kls kls.py")

from kls import *  # Import the functions and classes from kls script


class TestCircularList(unittest.TestCase):
    def setUp(self):
        self.circular_list = CircularList(['a', 'b', 'c'])

    def test_forward(self):
        self.circular_list.forward(1)
        self.assertEqual(self.circular_list.index, 1)

    def test_backward(self):
        self.circular_list.backward(1)
        self.assertEqual(self.circular_list.index, 2)  # Since it's circular, it goes to the end


class TestScriptFunctions(unittest.TestCase):
    @patch('kls.subprocess.check_output')
    def test_kubectl(self, mock_check_output):
        mock_check_output.return_value = b'pod1\npod2\npod3'
        result = kubectl('get pods')
        self.assertEqual(result, ['pod1', 'pod2', 'pod3'])


class TestMenu(unittest.TestCase):
    def setUp(self):
        self.rows = ['a', 'b', 'c']
        self.menu = Menu('Test', self.rows, 0, 10, 2)
        os.system("ln -s kls kls.py")

    def test_menu(self):
        self.assertEqual(self.menu.title, 'Test')
        self.assertEqual(self.menu.filtered_rows.elements, self.rows)
        self.assertEqual(self.menu.visible_rows(), ['a', 'b'])
        self.assertEqual(self.menu.selected_row(), 'a')

    def test_filter_rows_with_filter(self):
        # Apply a filter and test
        self.menu.filter = 'a'
        self.menu.filtered_rows = CircularList([x for x in self.menu.rows if self.menu.filter in x])
        self.assertEqual(self.menu.filtered_rows.elements, ['a'])

    def test_filter_rows_with_nonexistent_filter(self):
        # Apply a filter that matches no rows
        self.menu.filter = 'nonexistent'
        self.menu.filtered_rows = CircularList([x for x in self.menu.rows if self.menu.filter in x])
        self.assertEqual(self.menu.filtered_rows.elements, [])

    def tearDown(self):
        # Remove the symlink after each test
        os.unlink('kls.py')
        os.system("reset")


if __name__ == '__main__':
    unittest.main()