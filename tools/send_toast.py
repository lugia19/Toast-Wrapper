"""Sends a text to toast_wrapper.dll's test socket (DLL built with --features socket) and prints the reply
(1 = queued). Usage: python send_toast.py [--seconds N] <text...>"""
import socket
import sys

args = sys.argv[1:]
if args[:1] == ["--seconds"]:
    command = f"showfor {args[1]} " + " ".join(args[2:])
else:
    command = "show " + " ".join(args)

with socket.create_connection(("127.0.0.1", 48700), timeout=5) as connection:
    connection.sendall((command + "\n").encode("utf-8"))
    print(connection.makefile(encoding="utf-8").readline().strip())
