
<table>
<tr>
<td><img src="https://free.picui.cn/free/2025/07/03/686561024494b.png" alt="图片1" width="500"/></td>
<td><img src="https://free.picui.cn/free/2025/07/03/68656101abd01.png" alt="图片2" width="500"/></td>
</tr>
</table>

# Strange room

一个基于 WebRTC 的 P2P 网页匿名聊天室

## 特性亮点

- 完全匿名：无需注册账号，点对点通信不留痕迹
- 实时通信：基于 WebRTC，消息毫秒级送达
- 简单易用：界面简洁，操作直观
- 群聊机制：支持多人同时在线

## 技术栈

前端：Next.js + Tailwind CSS + Ant Design  
后端：Node.js + PeerJS

## 快速开始

1. 克隆项目

   ```bash
   git clone https://github.com/BlkSword/Strange-room.git
   cd Strange-room
   ```

2. 安装依赖

   ```bash
   npm install
   cd server && npm install
   ```

3. 启动后端服务

   ```bash
   cd server
   node index.js
   ```

4. 启动前端项目

   ```bash
   cd ..
   npm run build
   npm start
   ```

5. 打开浏览器访问 [http://localhost:3000](http://localhost:3000)

## 使用方法

1. 打开聊天室页面后，系统会自动分配一个唯一的 ID。
2. 将你的 ID 发送给好友，或输入对方的 ID 进行连接。
3. 连接成功后即可实时点对点聊天。
4. 所有通信均为端到端加密，不会被服务器记录。

## 许可证

本项目基于 Apache-2.0 开源，详见 LICENSE 文件。


