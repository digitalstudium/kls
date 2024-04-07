# KLS
## Description
`kls` is a cli tool for managing kubernetes cluster resources. Inspired by `lf` and `ranger` file managers. Written on python curses.
## Hotkeys
- `l` - logs of pod
- `g` - get yaml of resource
- `d` - describe resource
- `e` - edit resource

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

