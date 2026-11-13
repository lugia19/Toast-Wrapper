"""Sends one command to ac6_toast.dll's test socket (DLL built with --features socket) and prints the reply.
Usage: python send_toast.py show <seconds> <text...> | update <id> <text...> | dismiss <id>"""
import socket
import sys

with socket.create_connection(("127.0.0.1", 48700), timeout=5) as connection:
    connection.sendall((" ".join(sys.argv[1:]) + "\n").encode("utf-8"))
    print(connection.makefile(encoding="utf-8").readline().strip())
