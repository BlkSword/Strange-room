42
const express = require('express');
const app = express();
const server = require('http').createServer(app);
const io = require('socket.io')(server);

let players = {};   // 所有玩家 { id: { x, y, color } }
let bullets = [];   // 所有子弹 { x, y, ownerId }
let monsters = [];  // 所有怪物 { id, x, y }

// 模拟怪物生成
setInterval(() => {
    monsters = Array.from({ length: 5 }, (_, i) => ({
        id: `monster-${i}`,
        x: Math.random() * 780,
        y: Math.random() * 580
    }));
    io.emit("monsters", monsters);
}, 3000);

io.on("connection", (socket) => {
    console.log("新玩家加入:", socket.id);
    playerId = socket.id;

    // 初始化玩家
    players[socket.id] = {
        x: Math.random() * 780,
        y: Math.random() * 580,
        color: '#' + Math.floor(Math.random()*16777215).toString(16)
    };
    io.emit("players", players);
    io.emit("playerJoined", socket.id);

    // 接收移动指令
    socket.on("move", (data) => {
        players[socket.id].x = data.x;
        players[socket.id].y = data.y;
        io.emit("players", players);
    });

    // 接收射击指令
    socket.on("shoot", (bullet) => {
        bullet.id = Math.random().toString(36).substr(2, 9); // 生成子弹ID
        bullets.push(bullet);
        io.emit("bullets", bullets);
    });

    // 玩家断开连接
    socket.on("disconnect", () => {
        delete players[socket.id];
        io.emit("players", players);
        io.emit("playerLeft", socket.id);
    });
});

server.listen(3000, () => console.log("服务器运行在 http://localhost:3000"));