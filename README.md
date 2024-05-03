# KLS

## Description
`kls` is a cli tool based on `kubectl` for managing kubernetes cluster resources. 
Inspired by `lf` and `ranger` file managers, written in python. 
It is lightweight (~250 lines of code) and easy to customize. Supports mouse navigation as well as keyboard navigation.

## Key bindings
For kubectl (You can customize these bindings or add extra bindings in `KEY_BINDINGS` variable of `kls` in a row #4):
- `1` or `Enter` - get yaml of resource
- `2` - describe resource
- `3` - edit resource 
- `4` - logs of pod
- `5` - exec to pod
- `6` - network debug of pod (with nicolaka/netshoot container attached)
- `delete` - delete resource

Other:
- `Escape` - exit filter mode or `kls` itself
- `TAB`, arrow keys - navigation

![kls in action](./images/kls.gif)

## Dependencies
- `python3`
- `kubectl`
- `batcat`

## Installation
Download latest `kls`:
```
curl -O "https://git.digitalstudium.com/digitalstudium/kls/raw/branch/main/kls"
```
Then install it:
```
sudo install ./kls /usr/local/bin/
```

