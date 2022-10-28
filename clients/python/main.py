from socket import socket, SocketType, AF_INET, SOCK_STREAM
import json

SERVER_IP = "localhost"
SERVER_PORT = 7654

def main():
       sock = connect()
       auth(sock)
       play(sock)

def connect() -> SocketType:
       s: SocketType = socket(AF_INET, SOCK_STREAM)
       s.connect((SERVER_IP, SERVER_PORT))
       return s
       
def send(socket: SocketType, msg): # msg is a python object to be converted to json
       json_data = json.dumps(msg) + "\n"
       print(f"Sending: {json_data}")
       socket.sendall(bytes(json_data, 'utf-8'))
       
def recv(socket: SocketType): # Returns a json object
       print(f"Waiting for data from server")
       data = socket.makefile().readline()
       json_data = json.loads(data)
       print(f"Received: {json_data}")
       return json_data
       
def auth(socket: SocketType):
       send(socket, {'auth': {'username': 'example_python', 'password': 'kermit'}})
       
def play(socket: SocketType):
       x = 0
       while (True):
              state = recv(socket)
              try:
                     a = state["game-over"]
                     continue
              except:
                     pass
              
              # Based on the current game state, make a clever decision about my move
              move = {"move": {"x": x, "y": 2}}
              x = x + 1
              
              send(socket, move)

if __name__ == "__main__":
       main()