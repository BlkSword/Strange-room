42
require("dotenv").config();

const { ExpressPeerServer } = require("peer");
const express = require("express");
const cors = require("cors");
const fs = require("fs");
const https = require("https");

const app = express();

const PORT = 9000;

// 读取 SSL 证书
const sslOptions = {
  key: fs.readFileSync("/root/Strange-room/ssl/talk.blksword.com.key"),
  cert: fs.readFileSync("/root/Strange-room/ssl/talk.blksword.com.pem")
};

// 创建 HTTPS 服务器
const server = https.createServer(sslOptions, app);

app.use(express.static("public"));

const peerServer = ExpressPeerServer(server, {
  debug: true,
  allow_discovery: true,
});

app.use("/talk", peerServer);

server.listen(PORT, '0.0.0.0', () => {
  console.log(`PeerJS server running on HTTPS port ${PORT}`);
});