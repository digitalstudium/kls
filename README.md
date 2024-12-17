# KLS

## Description
`kls` is a cli tool based on `kubectl` for managing kubernetes cluster resources. 
Inspired by `lf` and `ranger` file managers, written in python. 

It is lightweight (~250 lines of code) and easy to customize.
Supports mouse navigation as well as keyboard navigation.

## Key bindings
### For kubectl 
You can customize these bindings or add extra bindings in `KEY_BINDINGS` variable of `kls` in a row #5:
- `g` - get yaml of resource
- `d` - describe resource
- `e` - edit resource 
- `l` - logs of pod
- `Enter` - exec to pod
- `n` - network debug of pod (with nicolaka/netshoot container attached)
- `delete` - delete resource

### Other:
- / - enter filter mode
- `Escape` - exit filter mode or `kls` itself
- `Backspace` - remove letter from filter
- `TAB`, arrow keys, `PgUp`, `PgDn`, `Home`, `End` - navigation

![kls in action](./images/kls.gif)

## Dependencies
- `python3`
- `kubectl`
- `bat`

## Installation
Install `batcat`:
```
sudo apt install bat -y
```
Download and install the latest `kls`:
```
curl -O "https://git.digitalstudium.com/digitalstudium/kls/raw/branch/main/kls" && sudo install ./kls /usr/local/bin/ && rm -f ./kls
```

