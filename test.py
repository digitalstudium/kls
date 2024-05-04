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
        self.rows = ['pods', 'services', 'configmaps']
        self.menu = Menu('Test Menu', self.rows, 0, 10)
        os.system("ln -s kls kls.py")

    def test_init(self):
        self.assertEqual(self.menu.title, 'Test Menu')
        self.assertEqual(self.menu.rows, self.rows)
        
    def test_filter_rows_no_filter(self):
        # Test with no filter applied
        self.assertEqual(self.menu.filtered_rows.elements, self.rows)

    def test_filter_rows_with_filter(self):
        # Apply a filter and test
        self.menu.filter = 'pod'
        self.menu.filtered_rows = CircularList([x for x in self.menu.rows if self.menu.filter in x])
        self.assertEqual(self.menu.filtered_rows.elements, ['pods'])

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