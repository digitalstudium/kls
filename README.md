# KLS
## Description
`kls` is a cli tool for managing kubernetes cluster resources. Inspired by `lf` and `ranger` file managers. Written on python curses.
## Key bindings
- `1` - get yaml of resource
- `2` - describe resource
- `3` - edit resource 
- `4` - logs of pod
- `a-zA-Z` - filter menu
- `Esc` - exit filter mode or exit `kls`

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

