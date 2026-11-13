"""Sends a text to toast_wrapper.dll's test socket (DLL built with --features socket) and prints the reply
(1 = queued). Usage: python send_toast.py <text...>"""
import socket
import sys

with socket.create_connection(("127.0.0.1", 48700), timeout=5) as connection:
    connection.sendall(("show " + " ".join(sys.argv[1:]) + "\n").encode("utf-8"))
    print(connection.makefile(encoding="utf-8").readline().strip())
