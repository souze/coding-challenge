# Gomoku

[Gomoku](https://en.wikipedia.org/wiki/Gomoku) is a classic 5-in-a-row game. Normally played with 2 players, but in this version, any number of players.

The game consists of a limited grid (say 50x50 squares). Each player takes turn to place one of their own pieces. You can only place your piece on an empty spot.

The game ends when one player has 5 connecting pieces, either diagonally or ortogonally. The game also ends of the board is filled with pieces but no player has won (draw).


## Protocol

### Game state

```json
{"cells":[{"occupied":name|"empty"}],"width":width,"height":height}
```

Where
 * *name* is the name of the player occupying this space
 * *width* is the width of the game board (will never change during a running game)
 * *height* is the height of the game board (will never change during a running game)
 * *cells* is a list of length width*height, the first *width* elements represents the first row, the following *width* elements represents the next row etc.


#### Example

The following would illustrate a 3x3 board (impossible to win).

e|_|s
---|---|---
e|o|_
_|_|o

```json
{"cells":[{"occupied":"erik"},"empty", {"occupied":"simon"},{"occupied":"erik"},{"occupied":"octopus"},"empty","empty","empty",{"occupied": "octopus"}],"width":3,"height":3}
```