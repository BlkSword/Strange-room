/**
 * 倒计时 Hook
 */

import { useState, useEffect, useCallback, useRef } from 'react';

export interface UseCountdownOptions {
  duration: number; // 毫秒
  onExpire?: () => void;
  onTick?: (remaining: number) => void;
  autoStart?: boolean;
}

export function useCountdown(options: UseCountdownOptions) {
  const [remaining, setRemaining] = useState(options.duration);
  const [isRunning, setIsRunning] = useState(options.autoStart || false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // 启动倒计时
  const start = useCallback(() => {
    if (isRunning) return;
    setIsRunning(true);
  }, [isRunning]);

  // 暂停倒计时
  const pause = useCallback(() => {
    setIsRunning(false);
  }, []);

  // 重置倒计时
  const reset = useCallback((newDuration?: number) => {
    setRemaining(newDuration ?? options.duration);
  }, [options.duration]);

  // 倒计时逻辑
  useEffect(() => {
    if (!isRunning) {
      if (intervalRef.current !== null) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
      return;
    }

    const interval = setInterval(() => {
      setRemaining((prev) => {
        const newRemaining = prev - 1000;

        // 触发 tick 回调
        options.onTick?.(newRemaining);

        // 检查是否过期
        if (newRemaining <= 0) {
          setIsRunning(false);
          options.onExpire?.();
          return 0;
        }

        return newRemaining;
      });
    }, 1000);

    intervalRef.current = interval;

    return () => {
      clearInterval(interval);
      intervalRef.current = null;
    };
  }, [isRunning, options]);

  // 格式化剩余时间
  const formatRemaining = useCallback((ms: number): string => {
    if (ms <= 0) return '00:00:00';

    const totalSeconds = Math.floor(ms / 1000);
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;

    if (hours > 0) {
      return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
    } else {
      return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
    }
  }, []);

  // 获取剩余时间百分比
  const getPercent = useCallback((): number => {
    return (remaining / options.duration) * 100;
  }, [remaining, options.duration]);

  // 是否即将结束（剩余时间小于10%）
  const isNearEnd = useCallback((): boolean => {
    return getPercent() < 10;
  }, [getPercent]);

  // 是否紧急（剩余时间小于1分钟）
  const isUrgent = useCallback((): boolean => {
    return remaining < 60 * 1000;
  }, [remaining]);

  return {
    remaining,
    isRunning,
    formatRemaining,
    getPercent,
    isNearEnd: isNearEnd(),
    isUrgent: isUrgent(),
    start,
    pause,
    reset,
  };
}
