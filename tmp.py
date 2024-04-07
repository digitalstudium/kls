from subprocess import call

call("kubectl edit deploy -n kube-system traefik", shell=True)
