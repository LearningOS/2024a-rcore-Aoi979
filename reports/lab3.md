# 总结







# 问答作业

stride 算法深入

stride 算法原理非常简单，但是有一个比较大的问题。例如两个 pass = 10 的进程，使用 8bit 无符号整形储存 stride， p1.stride = 255, p2.stride = 250，在 p2 执行一个时间片后，理论上下一次应该 p1 执行。

- 实际情况是轮到 p1 执行吗？为什么？

  答：不是，因为溢出，实际上p2.stride被更新为4，算法继续选出stride最小的进程来运行

我们之前要求进程优先级 >= 2 其实就是为了解决这个问题。可以证明， **在不考虑溢出的情况下** , 在进程优先级全部 >= 2 的情况下，如果严格按照算法执行，那么 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。

- 为什么？尝试简单说明（不要求严格证明）。

  答：priority>=2,pass<= BigStride/2,而stride_max不可能比stride_min多调度2次，算法保证了每次都调度stride最小的进程，所以STRIDE_MAX – STRIDE_MIN <= Pass_max = BigStride/2

- 已知以上结论，**考虑溢出的情况下**，可以为 Stride 设计特别的比较器，让 BinaryHeap<Stride> 的 pop 方法能返回真正最小的 Stride。补全下列代码中的 `partial_cmp` 函数，假设两个 Stride 永远不会相等。

```rust
use core::cmp::Ordering;

struct Stride(u64);

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
     if self.0.abs_diff(other.0) > (u64::MAX /2 ){
         Some(self.0.cmp(&other.0).reverse())
        }else{
         Some(self.0.cmp(&other.0))
        }
    }
}

impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        false
    }
}
```

TIPS: 使用 8 bits 存储 stride, BigStride = 255, 则: `(125 < 255) == false`, `(129 < 255) == true`.