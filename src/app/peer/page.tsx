"use client"

import { useEffect, useRef, useState } from 'react';
import Peer from 'peerjs';

const PeerPage = () => {
    const [peerInstance, setPeerInstance] = useState<Peer | null>(null);
    const [myUniqueId, setMyUniqueId] = useState<string>("");
    const [idToCall, setIdToCall] = useState('');
    const [message, setMessage] = useState('');
    const [chatHistory, setChatHistory] = useState<string[]>([]);
    const [dataConnection, setDataConnection] = useState<any>(null);

    const generateRandomString = () => Math.random().toString(36).substring(2);

    const handleConnect = () => {
        if (peerInstance && idToCall) {
            const conn = peerInstance.connect(idToCall);
            setDataConnection(conn);
            
            conn.on('data', (data: any) => {
                setChatHistory(prev => [...prev, `收到: ${data}`]);
            });

            conn.on('open', () => {
                setChatHistory(prev => [...prev, '连接已建立']);
            });
        }
    };

    const sendMessage = () => {
        if (dataConnection && message) {
            dataConnection.send(message);
            setChatHistory(prev => [...prev, `发送: ${message}`]);
            setMessage('');
        }
    };

    useEffect(() => {
        if (myUniqueId) {
            let peer: Peer;
            if (typeof window !== 'undefined') {
                peer = new Peer(myUniqueId, {
                    host: 'localhost',
                    port: 9000,
                    path: '/myapp',
                });

                setPeerInstance(peer);

                // 处理入站连接
                peer.on('connection', (conn) => {
                    setDataConnection(conn);
                    conn.on('data', (data: any) => {
                        setChatHistory(prev => [...prev, `收到: ${data}`]);
                    });

                    conn.on('open', () => {
                        setChatHistory(prev => [...prev, '连接已建立']);
                    });
                });
            }
            return () => {
                if (peer) {
                    peer.destroy();
                }
            };
        }
    }, [myUniqueId]);

    useEffect(() => {
        setMyUniqueId(generateRandomString);
    }, [])

    return (
        <div className='flex flex-col justify-center items-center p-12'>
            <p>你的ID: {myUniqueId}</p>
            
            <input 
                className='text-black mb-2'
                placeholder="要连接的ID"
                value={idToCall} 
                onChange={e => setIdToCall(e.target.value)} 
            />
            <button onClick={handleConnect}>连接</button>
            
            <div className='mt-4 w-full max-w-md h-64 overflow-y-auto border border-gray-300 p-2'>
                {chatHistory.map((msg, i) => (
                    <div key={i} className='mb-1'>{msg}</div>
                ))}
            </div>
            
            <div className='flex mt-2'>
                <input
                    className='text-black flex-grow'
                    placeholder='输入消息'
                    value={message}
                    onChange={e => setMessage(e.target.value)}
                    onKeyPress={(e) => e.key === 'Enter' && sendMessage()}
                />
                <button onClick={sendMessage} className='ml-2'>发送</button>
            </div>
        </div>
    );
};

export default PeerPage;
