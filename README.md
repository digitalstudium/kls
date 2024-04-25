# KLS

## Description
`kls` is a cli tool for managing kubernetes cluster resources. Inspired by `lf` and `ranger` file managers. 
It is lightweight and easy to customize. Supports mouse navigation as well as keyboard navigation.

## Key bindings for kubectl
- `1` - get yaml of resource
- `2` - describe resource
- `3` - edit resource 
- `4` - logs of pod
- `5` - exec to pod
- `6` - network debug of pod (with nicolaka/netshoot container attached)
- `delete` - delete resource

You can customize these bindings or add extra bindings in `KEY_BINDINGS` variable of `kls` (in a row #4).

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

