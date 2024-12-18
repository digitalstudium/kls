# KLS

## Description
`kls` is a cli tool based on `kubectl` for managing kubernetes cluster resources. 
Inspired by `lf` and `ranger` file managers, written in python. 

It is lightweight (~400 lines of code) and easy to customize.
Supports keyboard navigation and mouse navigation could be enabled (set MOUSE_ENABLED=True in a line #54).

## Key bindings
### For kubectl 
You can customize these bindings or add extra bindings in `KEY_BINDINGS` variable of `kls` in a line #10:
- `Ctrl+y` - get yaml of resource
- `Ctrl+d` - describe resource
- `Ctrl+e` - edit resource 
- `Ctrl+l` - logs of pod
- `Ctrl+x` - exec into pod
- `Ctrl+n` - network debug of pod (with nicolaka/netshoot container attached)
- `delete` - delete resource

### Other:
- `/` - enter filter mode
- `Escape` - exit filter mode or `kls` itself
- `Backspace` - remove letter from filter
- `TAB`, arrow keys, `PgUp`, `PgDn`, `Home`, `End` - navigation

![kls in action](./images/kls.gif)

## Dependencies
- `python3`
- `kubectl`
- `bat` - yaml viewer
- `lnav` - log viewer
- `yq` - yaml manipulation

## Installation
Install `batcat`:
```
sudo apt install bat lnav yq -y
```
Download and install the latest `kls`:
```
curl -O "https://git.digitalstudium.com/digitalstudium/kls/raw/branch/main/kls" && sudo install ./kls /usr/local/bin/ && rm -f ./kls
```

