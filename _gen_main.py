#
!
/
u
s
r
/
b
i
n
/
e
n
v
 
p
y
t
h
o
n
3


"
"
"
G
e
n
e
r
a
t
e
 
m
a
i
n
.
r
s
 
f
o
r
 
r
x
t
 
p
r
o
j
e
c
t
"
"
"




c
o
n
t
e
n
t
 
=
 
r
'
'
'
u
s
e
 
c
l
a
p
:
:
{
P
a
r
s
e
r
,
 
S
u
b
c
o
m
m
a
n
d
}
;


u
s
e
 
s
t
d
:
:
p
a
t
h
:
:
{
P
a
t
h
,
 
P
a
t
h
B
u
f
}
;




#
[
d
e
r
i
v
e
(
P
a
r
s
e
r
)
]


#
[
c
o
m
m
a
n
d
(
n
a
m
e
 
=
 
"
r
x
t
"
,
 
v
e
r
s
i
o
n
,
 
a
b
o
u
t
 
=
 
"
R
u
s
t
 
C
o
d
e
x
 
T
o
o
l
s
 
-
 
A
I
'
s
 
C
r
o
s
s
-
P
l
a
t
f
o
r
m
 
I
D
E
"
)
]


s
t
r
u
c
t
 
C
l
i
 
{


 
 
 
 
#
[
c
o
m
m
a
n
d
(
s
u
b
c
o
m
m
a
n
d
)
]


 
 
 
 
c
o
m
m
a
n
d
:
 
C
o
m
m
a
n
d
,


}




#
[
d
e
r
i
v
e
(
S
u
b
c
o
m
m
a
n
d
)
]


e
n
u
m
 
C
o
m
m
a
n
d
 
{


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
块
替
换
"
)
]


 
 
 
 
R
e
p
l
a
c
e
 
{


 
 
 
 
 
 
 
 
t
a
r
g
e
t
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
o
l
d
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
n
e
w
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
a
l
l
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
p
r
e
v
i
e
w
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
n
u
m
_
a
r
g
s
 
=
 
0
.
.
)
]
 
c
o
n
t
e
n
t
:
 
V
e
c
<
S
t
r
i
n
g
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
读
文
件
，
自
动
检
测
编
码
/
换
行
符
/
B
O
M
，
内
部
统
一
 
U
T
F
-
8
+
L
F
"
)
]


 
 
 
 
R
e
a
d
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
e
n
c
o
d
i
n
g
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
n
u
m
b
e
r
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
H
'
,
 
l
o
n
g
)
]
 
h
e
a
d
:
 
O
p
t
i
o
n
<
u
s
i
z
e
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
T
'
,
 
l
o
n
g
)
]
 
t
a
i
l
:
 
O
p
t
i
o
n
<
u
s
i
z
e
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
L
'
,
 
l
o
n
g
)
]
 
l
i
n
e
s
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
j
s
o
n
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
写
文
件
，
自
动
保
持
目
标
文
件
格
式
"
)
]


 
 
 
 
W
r
i
t
e
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
n
u
m
_
a
r
g
s
 
=
 
0
.
.
)
]
 
c
o
n
t
e
n
t
:
 
V
e
c
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
a
p
p
e
n
d
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
f
i
l
e
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
b
6
4
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
_
t
 
=
 
t
r
u
e
)
]
 
p
r
e
s
e
r
v
e
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
打
印
文
件
内
容
"
)
]


 
 
 
 
C
a
t
 
{
 
p
a
t
h
:
 
P
a
t
h
B
u
f
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
解
析
 
C
o
d
e
x
 
会
话
 
J
S
O
N
L
"
)
]


 
 
 
 
J
s
o
n
l
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
L
'
,
 
l
o
n
g
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
1
0
"
)
]
 
l
a
s
t
:
 
u
s
i
z
e
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
j
s
o
n
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
补
丁
工
具
"
)
]


 
 
 
 
P
a
t
c
h
 
{


 
 
 
 
 
 
 
 
p
a
t
h
s
:
 
V
e
c
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
r
e
v
e
r
s
e
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
c
h
e
c
k
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
o
u
t
p
u
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
文
件
元
信
息
 
+
 
文
件
指
纹
"
)
]


 
 
 
 
S
t
a
t
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
j
s
o
n
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
智
能
搜
索
"
)
]


 
 
 
 
F
i
n
d
 
{


 
 
 
 
 
 
 
 
q
u
e
r
y
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
p
a
t
h
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
n
'
,
 
l
o
n
g
 
=
 
"
n
a
m
e
"
)
]
 
n
a
m
e
_
p
a
t
t
e
r
n
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
t
'
,
 
l
o
n
g
 
=
 
"
t
y
p
e
"
)
]
 
f
i
l
e
_
t
y
p
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
C
'
,
 
l
o
n
g
 
=
 
"
c
o
n
t
e
x
t
"
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
2
"
)
]
 
c
o
n
t
e
x
t
:
 
u
s
i
z
e
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
c
a
s
e
_
s
e
n
s
i
t
i
v
e
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
c
o
u
n
t
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
s
t
a
t
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
r
e
p
l
a
c
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
 
=
 
"
w
i
t
h
"
)
]
 
r
e
p
l
a
c
e
_
w
i
t
h
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
p
r
e
v
i
e
w
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
代
码
结
构
分
析
"
)
]


 
 
 
 
S
t
r
u
c
t
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
f
u
n
c
t
i
o
n
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
t
y
p
e
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
d
e
e
p
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
e
x
t
r
a
c
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
差
异
对
比
"
)
]


 
 
 
 
D
i
f
f
 
{


 
 
 
 
 
 
 
 
f
i
r
s
t
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
s
e
c
o
n
d
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
C
'
,
 
l
o
n
g
 
=
 
"
c
o
n
t
e
x
t
"
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
3
"
)
]
 
c
o
n
t
e
x
t
:
 
u
s
i
z
e
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
s
t
a
t
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
依
赖
分
析
"
)
]


 
 
 
 
D
e
p
 
{


 
 
 
 
 
 
 
 
t
a
r
g
e
t
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
t
r
e
e
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
j
s
o
n
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
c
h
e
c
k
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
安
全
替
换
 
—
 
格
式
保
持
"
)
]


 
 
 
 
S
e
d
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
p
a
t
t
e
r
n
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
r
e
p
l
a
c
e
m
e
n
t
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
p
r
e
v
i
e
w
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
l
i
n
e
:
 
O
p
t
i
o
n
<
u
s
i
z
e
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
增
强
搜
索
 
—
 
跨
文
件
 
g
r
e
p
"
)
]


 
 
 
 
G
r
e
p
 
{


 
 
 
 
 
 
 
 
p
a
t
t
e
r
n
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
.
"
)
]
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
C
'
,
 
l
o
n
g
 
=
 
"
c
o
n
t
e
x
t
"
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
2
"
)
]
 
c
o
n
t
e
x
t
:
 
u
s
i
z
e
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
t
'
,
 
l
o
n
g
 
=
 
"
t
y
p
e
"
)
]
 
f
i
l
e
_
t
y
p
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
c
o
u
n
t
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
i
n
v
e
r
t
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
j
s
o
n
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
执
行
内
联
 
P
y
t
h
o
n
"
)
]


 
 
 
 
P
y
 
{


 
 
 
 
 
 
 
 
c
o
d
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
f
i
l
e
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
星
枢
记
忆
"
)
]


 
 
 
 
M
e
m
 
{
 
#
[
c
o
m
m
a
n
d
(
s
u
b
c
o
m
m
a
n
d
)
]
 
a
c
t
i
o
n
:
 
M
e
m
A
c
t
i
o
n
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
目
录
树
"
)
]


 
 
 
 
T
r
e
e
 
{


 
 
 
 
 
 
 
 
#
[
a
r
g
(
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
.
"
)
]
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
L
'
,
 
l
o
n
g
 
=
 
"
d
e
p
t
h
"
)
]
 
d
e
p
t
h
:
 
O
p
t
i
o
n
<
u
s
i
z
e
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
I
'
,
 
l
o
n
g
 
=
 
"
i
g
n
o
r
e
"
)
]
 
i
g
n
o
r
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
d
'
,
 
l
o
n
g
 
=
 
"
d
i
r
s
-
o
n
l
y
"
)
]
 
d
i
r
s
_
o
n
l
y
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
智
能
 
G
i
t
 
提
交
"
)
]


 
 
 
 
C
o
m
m
i
t
 
{


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
m
e
s
s
a
g
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
a
l
l
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
d
r
y
_
r
u
n
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
J
S
O
N
 
查
询
/
格
式
化
"
)
]


 
 
 
 
J
q
 
{


 
 
 
 
 
 
 
 
q
u
e
r
y
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
f
i
l
e
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
f
m
t
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
c
o
m
p
a
c
t
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
结
构
化
文
件
编
辑
 
—
 
格
式
保
持
"
)
]


 
 
 
 
E
d
i
t
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
a
f
t
e
r
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
b
e
f
o
r
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
d
e
l
e
t
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
r
e
p
l
a
c
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
n
u
m
_
a
r
g
s
 
=
 
0
.
.
)
]
 
c
o
n
t
e
n
t
:
 
V
e
c
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
p
r
e
v
i
e
w
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
s
c
r
i
p
t
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
文
件
哈
希
"
)
]


 
 
 
 
H
a
s
h
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
s
h
a
2
5
6
"
)
]
 
a
l
g
o
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
t
e
x
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
U
U
I
D
 
生
成
器
"
)
]


 
 
 
 
U
u
i
d
 
{
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
_
t
 
=
 
1
)
]
 
c
o
u
n
t
:
 
u
s
i
z
e
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
编
码
/
解
码
"
)
]


 
 
 
 
E
n
c
 
{


 
 
 
 
 
 
 
 
m
o
d
e
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
i
n
p
u
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
d
e
c
o
d
e
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
f
i
l
e
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
解
码
"
)
]


 
 
 
 
D
e
c
 
{


 
 
 
 
 
 
 
 
m
o
d
e
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
i
n
p
u
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
f
i
l
e
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
文
件
监
听
"
)
]


 
 
 
 
W
a
t
c
h
 
{


 
 
 
 
 
 
 
 
p
a
t
t
e
r
n
s
:
 
V
e
c
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
c
m
d
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
p
'
,
 
l
o
n
g
 
=
 
"
p
a
t
h
"
)
]
 
p
a
t
h
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
d
'
,
 
l
o
n
g
 
=
 
"
d
e
b
o
u
n
c
e
"
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
5
0
0
"
)
]
 
d
e
b
o
u
n
c
e
:
 
u
6
4
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
命
令
计
时
"
)
]


 
 
 
 
T
i
m
e
 
{
 
c
m
d
:
 
S
t
r
i
n
g
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
多
语
言
代
码
执
行
"
)
]


 
 
 
 
E
x
e
c
 
{


 
 
 
 
 
 
 
 
c
o
d
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
b
6
4
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
l
a
n
g
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
w
r
i
t
e
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
f
i
l
e
:
 
O
p
t
i
o
n
<
P
a
t
h
B
u
f
>
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
行
排
序
"
)
]


 
 
 
 
S
o
r
t
 
{


 
 
 
 
 
 
 
 
i
n
p
u
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
,
 
l
o
n
g
)
]
 
r
e
v
e
r
s
e
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
n
'
,
 
l
o
n
g
)
]
 
n
u
m
e
r
i
c
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
k
'
,
 
l
o
n
g
)
]
 
c
o
l
u
m
n
:
 
O
p
t
i
o
n
<
u
s
i
z
e
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
t
'
,
 
l
o
n
g
)
]
 
s
e
p
a
r
a
t
o
r
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
u
'
,
 
l
o
n
g
)
]
 
u
n
i
q
u
e
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
行
去
重
"
)
]


 
 
 
 
U
n
i
q
 
{


 
 
 
 
 
 
 
 
i
n
p
u
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
c
'
,
 
l
o
n
g
)
]
 
c
o
u
n
t
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
d
'
,
 
l
o
n
g
)
]
 
d
u
p
l
i
c
a
t
e
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
i
'
,
 
l
o
n
g
)
]
 
i
g
n
o
r
e
_
c
a
s
e
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
列
提
取
"
)
]


 
 
 
 
C
u
t
 
{


 
 
 
 
 
 
 
 
i
n
p
u
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
d
'
,
 
l
o
n
g
)
]
 
d
e
l
i
m
i
t
e
r
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
f
'
,
 
l
o
n
g
,
 
r
e
q
u
i
r
e
d
 
=
 
t
r
u
e
)
]
 
f
i
e
l
d
s
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
s
'
,
 
l
o
n
g
)
]
 
o
n
l
y
_
d
e
l
i
m
i
t
e
d
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
行
/
词
/
字
符
/
字
节
统
计
"
)
]


 
 
 
 
C
o
u
n
t
 
{


 
 
 
 
 
 
 
 
i
n
p
u
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
l
'
,
 
l
o
n
g
)
]
 
l
i
n
e
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
w
'
,
 
l
o
n
g
)
]
 
w
o
r
d
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
m
'
,
 
l
o
n
g
)
]
 
c
h
a
r
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
c
'
,
 
l
o
n
g
)
]
 
b
y
t
e
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
L
'
,
 
l
o
n
g
)
]
 
m
a
x
_
l
i
n
e
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
智
能
 
R
u
s
t
 
构
建
"
)
]


 
 
 
 
B
u
i
l
d
 
{


 
 
 
 
 
 
 
 
d
i
r
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
t
'
,
 
l
o
n
g
 
=
 
"
t
a
r
g
e
t
"
)
]
 
t
a
r
g
e
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
p
'
,
 
l
o
n
g
 
=
 
"
p
r
o
f
i
l
e
"
)
]
 
p
r
o
f
i
l
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
b
'
,
 
l
o
n
g
 
=
 
"
b
i
n
"
)
]
 
b
i
n
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
f
e
a
t
u
r
e
s
:
 
V
e
c
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
w
o
r
k
s
p
a
c
e
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
l
i
s
t
_
t
a
r
g
e
t
s
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
n
o
_
c
o
n
f
i
g
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
R
u
s
t
 
代
码
质
量
检
查
"
)
]


 
 
 
 
C
h
e
c
k
 
{


 
 
 
 
 
 
 
 
d
i
r
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
c
l
i
p
p
y
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
f
m
t
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
f
i
x
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
编
译
产
物
大
小
分
析
"
)
]


 
 
 
 
S
i
z
e
 
{


 
 
 
 
 
 
 
 
d
i
r
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
t
'
,
 
l
o
n
g
 
=
 
"
t
a
r
g
e
t
"
)
]
 
t
a
r
g
e
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
p
'
,
 
l
o
n
g
 
=
 
"
p
r
o
f
i
l
e
"
)
]
 
p
r
o
f
i
l
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
a
'
,
 
l
o
n
g
)
]
 
a
l
l
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
H
'
,
 
l
o
n
g
)
]
 
h
u
m
a
n
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
s
'
,
 
l
o
n
g
)
]
 
s
o
r
t
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
智
能
清
理
"
)
]


 
 
 
 
C
l
e
a
n
 
{


 
 
 
 
 
 
 
 
d
i
r
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
t
'
,
 
l
o
n
g
 
=
 
"
t
a
r
g
e
t
"
)
]
 
t
a
r
g
e
t
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
p
'
,
 
l
o
n
g
 
=
 
"
p
r
o
f
i
l
e
"
)
]
 
p
r
o
f
i
l
e
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
d
r
y
_
r
u
n
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
a
'
,
 
l
o
n
g
)
]
 
a
l
l
:
 
b
o
o
l
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
文
件
格
式
统
一
 
—
 
跨
平
台
文
本
标
准
化
"
)
]


 
 
 
 
N
o
r
m
a
l
i
z
e
 
{


 
 
 
 
 
 
 
 
p
a
t
h
:
 
P
a
t
h
B
u
f
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
e
'
,
 
l
o
n
g
 
=
 
"
e
n
d
i
n
g
"
)
]
 
e
n
d
i
n
g
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
r
e
m
o
v
e
_
b
o
m
:
 
b
o
o
l
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
)
]
 
j
s
o
n
:
 
b
o
o
l
,


 
 
 
 
}
,


}




#
[
d
e
r
i
v
e
(
S
u
b
c
o
m
m
a
n
d
)
]


e
n
u
m
 
M
e
m
A
c
t
i
o
n
 
{


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
保
存
记
忆
"
)
]


 
 
 
 
S
a
v
e
 
{


 
 
 
 
 
 
 
 
c
o
n
t
e
n
t
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
c
o
d
e
"
)
]
 
c
a
t
e
g
o
r
y
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
l
o
n
g
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
_
t
 
=
 
0
.
6
)
]
 
i
m
p
o
r
t
a
n
c
e
:
 
f
6
4
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
搜
索
记
忆
"
)
]


 
 
 
 
S
e
a
r
c
h
 
{


 
 
 
 
 
 
 
 
q
u
e
r
y
:
 
S
t
r
i
n
g
,


 
 
 
 
 
 
 
 
#
[
a
r
g
(
s
h
o
r
t
 
=
 
'
k
'
,
 
l
o
n
g
 
=
 
"
t
o
p
-
k
"
,
 
d
e
f
a
u
l
t
_
v
a
l
u
e
 
=
 
"
5
"
)
]
 
t
o
p
_
k
:
 
u
s
i
z
e
,


 
 
 
 
}
,


 
 
 
 
#
[
c
o
m
m
a
n
d
(
a
b
o
u
t
 
=
 
"
星
枢
统
计
"
)
]


 
 
 
 
S
t
a
t
s
,


}




m
o
d
 
s
i
g
n
a
t
u
r
e
;


m
o
d
 
r
e
p
l
a
c
e
;


m
o
d
 
r
e
a
d
;


m
o
d
 
w
r
i
t
e
;


m
o
d
 
c
a
t
;


m
o
d
 
j
s
o
n
l
;


m
o
d
 
s
t
a
t
;


m
o
d
 
f
i
n
d
;


#
[
p
a
t
h
 
=
 
"
s
t
r
u
c
t
.
r
s
"
]


m
o
d
 
s
t
r
u
c
t
_
m
o
d
;


m
o
d
 
d
i
f
f
;


m
o
d
 
d
e
p
;


m
o
d
 
s
e
d
;


m
o
d
 
g
r
e
p
;


m
o
d
 
p
a
t
c
h
;


m
o
d
 
t
r
e
e
;


m
o
d
 
p
y
;


m
o
d
 
m
e
m
;


m
o
d
 
c
o
m
m
i
t
;


m
o
d
 
j
q
;


m
o
d
 
e
d
i
t
;


m
o
d
 
h
a
s
h
;


m
o
d
 
u
u
i
d
g
e
n
;


m
o
d
 
e
n
c
;


m
o
d
 
w
a
t
c
h
;


m
o
d
 
t
i
m
e
c
m
d
;


m
o
d
 
e
x
e
c
;


m
o
d
 
s
o
r
t
;


m
o
d
 
u
n
i
q
;


m
o
d
 
c
u
t
;


m
o
d
 
c
o
u
n
t
;


m
o
d
 
b
u
i
l
d
;


m
o
d
 
c
h
e
c
k
;


m
o
d
 
s
i
z
e
;


m
o
d
 
c
l
e
a
n
;


m
o
d
 
n
o
r
m
a
l
i
z
e
;




f
n
 
m
a
i
n
(
)
 
-
>
 
a
n
y
h
o
w
:
:
R
e
s
u
l
t
<
(
)
>
 
{


 
 
 
 
l
e
t
 
c
l
i
 
=
 
C
l
i
:
:
p
a
r
s
e
(
)
;


 
 
 
 
m
a
t
c
h
 
c
l
i
.
c
o
m
m
a
n
d
 
{


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
R
e
p
l
a
c
e
 
{
 
t
a
r
g
e
t
,
 
o
l
d
,
 
n
e
w
,
 
a
l
l
,
 
p
r
e
v
i
e
w
,
 
c
o
n
t
e
n
t
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
n
c
:
 
O
p
t
i
o
n
<
S
t
r
i
n
g
>
 
=
 
i
f
 
l
e
t
 
S
o
m
e
(
f
)
 
=
 
n
e
w
 
{


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
S
o
m
e
(
s
t
d
:
:
f
s
:
:
r
e
a
d
_
t
o
_
s
t
r
i
n
g
(
f
)
?
)


 
 
 
 
 
 
 
 
 
 
 
 
}
 
e
l
s
e
 
i
f
 
!
c
o
n
t
e
n
t
.
i
s
_
e
m
p
t
y
(
)
 
{


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
S
o
m
e
(
c
o
n
t
e
n
t
.
j
o
i
n
(
"
\
n
"
)
)


 
 
 
 
 
 
 
 
 
 
 
 
}
 
e
l
s
e
 
{
 
N
o
n
e
 
}
;


 
 
 
 
 
 
 
 
 
 
 
 
r
e
p
l
a
c
e
:
:
r
u
n
(
&
t
a
r
g
e
t
,
 
&
o
l
d
,
 
n
c
.
a
s
_
d
e
r
e
f
(
)
,
 
a
l
l
,
 
p
r
e
v
i
e
w
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
R
e
a
d
 
{
 
p
a
t
h
,
 
e
n
c
o
d
i
n
g
,
 
n
u
m
b
e
r
,
 
h
e
a
d
,
 
t
a
i
l
,
 
l
i
n
e
s
,
 
j
s
o
n
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
r
e
a
d
:
:
r
u
n
(
&
p
a
t
h
,
 
e
n
c
o
d
i
n
g
,
 
n
u
m
b
e
r
,
 
h
e
a
d
,
 
t
a
i
l
,
 
l
i
n
e
s
,
 
j
s
o
n
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
W
r
i
t
e
 
{
 
p
a
t
h
,
 
c
o
n
t
e
n
t
,
 
a
p
p
e
n
d
,
 
f
i
l
e
,
 
b
6
4
,
 
p
r
e
s
e
r
v
e
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
i
f
 
l
e
t
 
S
o
m
e
(
f
)
 
=
 
f
i
l
e
 
{


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
w
r
i
t
e
:
:
r
u
n
_
f
i
l
e
(
&
p
a
t
h
,
 
&
f
,
 
a
p
p
e
n
d
)
?
;


 
 
 
 
 
 
 
 
 
 
 
 
}
 
e
l
s
e
 
i
f
 
b
6
4
 
{


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
j
 
=
 
c
o
n
t
e
n
t
.
j
o
i
n
(
"
"
)
;


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
w
r
i
t
e
:
:
r
u
n
_
b
6
4
(
&
p
a
t
h
,
 
&
j
,
 
a
p
p
e
n
d
)
?
;


 
 
 
 
 
 
 
 
 
 
 
 
}
 
e
l
s
e
 
{


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
j
 
=
 
c
o
n
t
e
n
t
.
j
o
i
n
(
"
\
n
"
)
;


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
w
r
i
t
e
:
:
r
u
n
(
&
p
a
t
h
,
 
i
f
 
c
o
n
t
e
n
t
.
i
s
_
e
m
p
t
y
(
)
 
{
 
N
o
n
e
 
}
 
e
l
s
e
 
{
 
S
o
m
e
(
&
j
)
 
}
,
 
a
p
p
e
n
d
,
 
p
r
e
s
e
r
v
e
)
?
;


 
 
 
 
 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
C
a
t
 
{
 
p
a
t
h
 
}
 
=
>
 
c
a
t
:
:
r
u
n
(
&
p
a
t
h
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
J
s
o
n
l
 
{
 
p
a
t
h
,
 
l
a
s
t
,
 
j
s
o
n
 
}
 
=
>
 
j
s
o
n
l
:
:
r
u
n
(
&
p
a
t
h
,
 
l
a
s
t
,
 
j
s
o
n
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
S
t
a
t
 
{
 
p
a
t
h
,
 
j
s
o
n
 
}
 
=
>
 
s
t
a
t
:
:
r
u
n
(
&
p
a
t
h
,
 
j
s
o
n
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
F
i
n
d
 
{
 
q
u
e
r
y
,
 
p
a
t
h
,
 
n
a
m
e
_
p
a
t
t
e
r
n
,
 
f
i
l
e
_
t
y
p
e
,
 
c
o
n
t
e
x
t
,
 
c
a
s
e
_
s
e
n
s
i
t
i
v
e
,
 
c
o
u
n
t
,
 
s
t
a
t
s
,
 
r
e
p
l
a
c
e
,
 
r
e
p
l
a
c
e
_
w
i
t
h
,
 
p
r
e
v
i
e
w
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
f
i
n
d
:
:
r
u
n
(
q
u
e
r
y
.
a
s
_
d
e
r
e
f
(
)
,
 
p
a
t
h
.
a
s
_
d
e
r
e
f
(
)
,
 
n
a
m
e
_
p
a
t
t
e
r
n
.
a
s
_
d
e
r
e
f
(
)
,
 
f
i
l
e
_
t
y
p
e
.
a
s
_
d
e
r
e
f
(
)
,
 
c
o
n
t
e
x
t
,
 
c
a
s
e
_
s
e
n
s
i
t
i
v
e
,
 
c
o
u
n
t
,
 
s
t
a
t
s
,
 
r
e
p
l
a
c
e
.
a
s
_
d
e
r
e
f
(
)
,
 
r
e
p
l
a
c
e
_
w
i
t
h
.
a
s
_
d
e
r
e
f
(
)
,
 
p
r
e
v
i
e
w
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
S
t
r
u
c
t
 
{
 
p
a
t
h
,
 
f
u
n
c
t
i
o
n
s
,
 
t
y
p
e
s
,
 
d
e
e
p
,
 
e
x
t
r
a
c
t
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
s
t
r
u
c
t
_
m
o
d
:
:
r
u
n
(
&
p
a
t
h
,
 
f
u
n
c
t
i
o
n
s
,
 
t
y
p
e
s
,
 
d
e
e
p
,
 
e
x
t
r
a
c
t
.
a
s
_
d
e
r
e
f
(
)
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
D
i
f
f
 
{
 
f
i
r
s
t
,
 
s
e
c
o
n
d
,
 
c
o
n
t
e
x
t
,
 
s
t
a
t
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
d
i
f
f
:
:
r
u
n
(
&
f
i
r
s
t
,
 
s
e
c
o
n
d
.
a
s
_
d
e
r
e
f
(
)
,
 
c
o
n
t
e
x
t
,
 
s
t
a
t
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
D
e
p
 
{
 
t
a
r
g
e
t
,
 
t
r
e
e
,
 
j
s
o
n
,
 
c
h
e
c
k
 
}
 
=
>
 
d
e
p
:
:
r
u
n
(
&
t
a
r
g
e
t
,
 
t
r
e
e
,
 
j
s
o
n
,
 
c
h
e
c
k
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
S
e
d
 
{
 
p
a
t
h
,
 
p
a
t
t
e
r
n
,
 
r
e
p
l
a
c
e
m
e
n
t
,
 
p
r
e
v
i
e
w
,
 
l
i
n
e
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
s
e
d
:
:
r
u
n
(
&
p
a
t
h
,
 
&
p
a
t
t
e
r
n
,
 
&
r
e
p
l
a
c
e
m
e
n
t
,
 
p
r
e
v
i
e
w
,
 
l
i
n
e
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
G
r
e
p
 
{
 
p
a
t
t
e
r
n
,
 
p
a
t
h
,
 
c
o
n
t
e
x
t
,
 
f
i
l
e
_
t
y
p
e
,
 
c
o
u
n
t
,
 
i
n
v
e
r
t
,
 
j
s
o
n
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
g
r
e
p
:
:
r
u
n
(
&
p
a
t
t
e
r
n
,
 
&
p
a
t
h
,
 
c
o
n
t
e
x
t
,
 
f
i
l
e
_
t
y
p
e
.
a
s
_
d
e
r
e
f
(
)
,
 
c
o
u
n
t
,
 
i
n
v
e
r
t
,
 
j
s
o
n
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
P
a
t
c
h
 
{
 
p
a
t
h
s
,
 
r
e
v
e
r
s
e
,
 
c
h
e
c
k
,
 
o
u
t
p
u
t
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
p
a
t
c
h
:
:
r
u
n
(
&
p
a
t
h
s
,
 
r
e
v
e
r
s
e
,
 
c
h
e
c
k
,
 
o
u
t
p
u
t
.
a
s
_
d
e
r
e
f
(
)
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
P
y
 
{
 
c
o
d
e
,
 
f
i
l
e
 
}
 
=
>
 
p
y
:
:
r
u
n
(
c
o
d
e
.
a
s
_
d
e
r
e
f
(
)
,
 
f
i
l
e
.
a
s
_
r
e
f
(
)
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
M
e
m
 
{
 
a
c
t
i
o
n
 
}
 
=
>
 
m
a
t
c
h
 
a
c
t
i
o
n
 
{


 
 
 
 
 
 
 
 
 
 
 
 
M
e
m
A
c
t
i
o
n
:
:
S
a
v
e
 
{
 
c
o
n
t
e
n
t
,
 
c
a
t
e
g
o
r
y
,
 
i
m
p
o
r
t
a
n
c
e
 
}
 
=
>
 
m
e
m
:
:
r
u
n
_
s
a
v
e
(
&
c
o
n
t
e
n
t
,
 
&
c
a
t
e
g
o
r
y
,
 
i
m
p
o
r
t
a
n
c
e
)
?
,


 
 
 
 
 
 
 
 
 
 
 
 
M
e
m
A
c
t
i
o
n
:
:
S
e
a
r
c
h
 
{
 
q
u
e
r
y
,
 
t
o
p
_
k
 
}
 
=
>
 
m
e
m
:
:
r
u
n
_
s
e
a
r
c
h
(
&
q
u
e
r
y
,
 
t
o
p
_
k
)
?
,


 
 
 
 
 
 
 
 
 
 
 
 
M
e
m
A
c
t
i
o
n
:
:
S
t
a
t
s
 
=
>
 
m
e
m
:
:
r
u
n
_
s
t
a
t
s
(
)
?
,


 
 
 
 
 
 
 
 
}
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
T
r
e
e
 
{
 
p
a
t
h
,
 
d
e
p
t
h
,
 
i
g
n
o
r
e
,
 
d
i
r
s
_
o
n
l
y
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
i
g
n
o
r
e
s
:
 
V
e
c
<
S
t
r
i
n
g
>
 
=
 
i
g
n
o
r
e
.
a
s
_
d
e
r
e
f
(
)


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
.
m
a
p
(
|
s
|
 
s
.
s
p
l
i
t
(
'
,
'
)
.
m
a
p
(
|
x
|
 
x
.
t
r
i
m
(
)
.
t
o
_
s
t
r
i
n
g
(
)
)
.
c
o
l
l
e
c
t
(
)
)


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
.
u
n
w
r
a
p
_
o
r
_
d
e
f
a
u
l
t
(
)
;


 
 
 
 
 
 
 
 
 
 
 
 
t
r
e
e
:
:
r
u
n
(
&
p
a
t
h
,
 
d
e
p
t
h
,
 
&
i
g
n
o
r
e
s
,
 
d
i
r
s
_
o
n
l
y
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
C
o
m
m
i
t
 
{
 
m
e
s
s
a
g
e
,
 
a
l
l
,
 
d
r
y
_
r
u
n
 
}
 
=
>
 
c
o
m
m
i
t
:
:
r
u
n
(
m
e
s
s
a
g
e
.
a
s
_
d
e
r
e
f
(
)
,
 
a
l
l
,
 
d
r
y
_
r
u
n
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
J
q
 
{
 
q
u
e
r
y
,
 
f
i
l
e
,
 
f
m
t
,
 
c
o
m
p
a
c
t
 
}
 
=
>
 
j
q
:
:
r
u
n
(
q
u
e
r
y
.
a
s
_
d
e
r
e
f
(
)
,
 
f
i
l
e
.
a
s
_
d
e
r
e
f
(
)
,
 
f
m
t
,
 
c
o
m
p
a
c
t
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
E
d
i
t
 
{
 
p
a
t
h
,
 
a
f
t
e
r
,
 
b
e
f
o
r
e
,
 
d
e
l
e
t
e
,
 
r
e
p
l
a
c
e
,
 
c
o
n
t
e
n
t
,
 
p
r
e
v
i
e
w
,
 
s
c
r
i
p
t
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
r
e
p
 
=
 
r
e
p
l
a
c
e
.
a
s
_
d
e
r
e
f
(
)
.
a
n
d
_
t
h
e
n
(
|
s
|
 
{


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
m
u
t
 
p
 
=
 
s
.
s
p
l
i
t
n
(
2
,
 
'
,
'
)
;


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
S
o
m
e
(
(
p
.
n
e
x
t
(
)
?
,
 
p
.
n
e
x
t
(
)
?
)
)


 
 
 
 
 
 
 
 
 
 
 
 
}
)
;


 
 
 
 
 
 
 
 
 
 
 
 
i
f
 
l
e
t
 
S
o
m
e
(
s
p
)
 
=
 
s
c
r
i
p
t
 
{


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
e
d
i
t
:
:
r
u
n
_
s
c
r
i
p
t
(
&
p
a
t
h
,
 
&
s
p
,
 
p
r
e
v
i
e
w
)
?
;


 
 
 
 
 
 
 
 
 
 
 
 
}
 
e
l
s
e
 
{


 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
 
e
d
i
t
:
:
r
u
n
(
&
p
a
t
h
,
 
a
f
t
e
r
.
a
s
_
d
e
r
e
f
(
)
,
 
b
e
f
o
r
e
.
a
s
_
d
e
r
e
f
(
)
,
 
d
e
l
e
t
e
.
a
s
_
d
e
r
e
f
(
)
,
 
r
e
p
,
 
&
c
o
n
t
e
n
t
,
 
p
r
e
v
i
e
w
)
?
;


 
 
 
 
 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
H
a
s
h
 
{
 
p
a
t
h
,
 
a
l
g
o
,
 
t
e
x
t
 
}
 
=
>
 
h
a
s
h
:
:
r
u
n
(
p
a
t
h
.
a
s
_
d
e
r
e
f
(
)
,
 
&
a
l
g
o
,
 
t
e
x
t
.
a
s
_
d
e
r
e
f
(
)
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
U
u
i
d
 
{
 
c
o
u
n
t
 
}
 
=
>
 
u
u
i
d
g
e
n
:
:
r
u
n
(
c
o
u
n
t
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
E
n
c
 
{
 
m
o
d
e
,
 
i
n
p
u
t
,
 
d
e
c
o
d
e
,
 
f
i
l
e
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
f
c
;


 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
i
s
:
 
O
p
t
i
o
n
<
&
s
t
r
>
 
=
 
i
f
 
l
e
t
 
S
o
m
e
(
f
)
 
=
 
f
i
l
e
 
{
 
f
c
 
=
 
s
t
d
:
:
f
s
:
:
r
e
a
d
_
t
o
_
s
t
r
i
n
g
(
f
)
?
;
 
S
o
m
e
(
&
f
c
)
 
}
 
e
l
s
e
 
{
 
i
n
p
u
t
.
a
s
_
d
e
r
e
f
(
)
 
}
;


 
 
 
 
 
 
 
 
 
 
 
 
e
n
c
:
:
r
u
n
(
&
m
o
d
e
,
 
i
s
,
 
d
e
c
o
d
e
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
D
e
c
 
{
 
m
o
d
e
,
 
i
n
p
u
t
,
 
f
i
l
e
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
f
c
;


 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
i
s
:
 
O
p
t
i
o
n
<
&
s
t
r
>
 
=
 
i
f
 
l
e
t
 
S
o
m
e
(
f
)
 
=
 
f
i
l
e
 
{
 
f
c
 
=
 
s
t
d
:
:
f
s
:
:
r
e
a
d
_
t
o
_
s
t
r
i
n
g
(
f
)
?
;
 
S
o
m
e
(
&
f
c
)
 
}
 
e
l
s
e
 
{
 
i
n
p
u
t
.
a
s
_
d
e
r
e
f
(
)
 
}
;


 
 
 
 
 
 
 
 
 
 
 
 
e
n
c
:
:
r
u
n
(
&
m
o
d
e
,
 
i
s
,
 
t
r
u
e
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
W
a
t
c
h
 
{
 
p
a
t
t
e
r
n
s
,
 
c
m
d
,
 
p
a
t
h
,
 
d
e
b
o
u
n
c
e
 
}
 
=
>
 
w
a
t
c
h
:
:
r
u
n
(
&
p
a
t
t
e
r
n
s
,
 
&
c
m
d
,
 
p
a
t
h
.
a
s
_
d
e
r
e
f
(
)
,
 
d
e
b
o
u
n
c
e
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
T
i
m
e
 
{
 
c
m
d
 
}
 
=
>
 
t
i
m
e
c
m
d
:
:
r
u
n
(
&
c
m
d
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
E
x
e
c
 
{
 
c
o
d
e
,
 
b
6
4
,
 
l
a
n
g
,
 
w
r
i
t
e
,
 
f
i
l
e
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
l
e
t
 
c
s
 
=
 
i
f
 
l
e
t
 
S
o
m
e
(
f
)
 
=
 
f
i
l
e
 
{
 
s
t
d
:
:
f
s
:
:
r
e
a
d
_
t
o
_
s
t
r
i
n
g
(
f
)
?
 
}
 
e
l
s
e
 
{
 
c
o
d
e
.
u
n
w
r
a
p
_
o
r
_
d
e
f
a
u
l
t
(
)
 
}
;


 
 
 
 
 
 
 
 
 
 
 
 
e
x
e
c
:
:
r
u
n
(
&
c
s
,
 
b
6
4
,
 
l
a
n
g
.
a
s
_
d
e
r
e
f
(
)
,
 
w
r
i
t
e
.
a
s
_
r
e
f
(
)
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
S
o
r
t
 
{
 
i
n
p
u
t
,
 
r
e
v
e
r
s
e
,
 
n
u
m
e
r
i
c
,
 
c
o
l
u
m
n
,
 
s
e
p
a
r
a
t
o
r
,
 
u
n
i
q
u
e
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
s
o
r
t
:
:
r
u
n
(
i
n
p
u
t
.
a
s
_
d
e
r
e
f
(
)
,
 
r
e
v
e
r
s
e
,
 
n
u
m
e
r
i
c
,
 
c
o
l
u
m
n
,
 
s
e
p
a
r
a
t
o
r
,
 
u
n
i
q
u
e
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
U
n
i
q
 
{
 
i
n
p
u
t
,
 
c
o
u
n
t
,
 
d
u
p
l
i
c
a
t
e
s
,
 
i
g
n
o
r
e
_
c
a
s
e
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
u
n
i
q
:
:
r
u
n
(
i
n
p
u
t
.
a
s
_
d
e
r
e
f
(
)
,
 
c
o
u
n
t
,
 
d
u
p
l
i
c
a
t
e
s
,
 
i
g
n
o
r
e
_
c
a
s
e
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
C
u
t
 
{
 
i
n
p
u
t
,
 
d
e
l
i
m
i
t
e
r
,
 
f
i
e
l
d
s
,
 
o
n
l
y
_
d
e
l
i
m
i
t
e
d
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
c
u
t
:
:
r
u
n
(
i
n
p
u
t
.
a
s
_
d
e
r
e
f
(
)
,
 
d
e
l
i
m
i
t
e
r
,
 
&
f
i
e
l
d
s
,
 
o
n
l
y
_
d
e
l
i
m
i
t
e
d
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
C
o
u
n
t
 
{
 
i
n
p
u
t
,
 
l
i
n
e
s
,
 
w
o
r
d
s
,
 
c
h
a
r
s
,
 
b
y
t
e
s
,
 
m
a
x
_
l
i
n
e
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
c
o
u
n
t
:
:
r
u
n
(
i
n
p
u
t
.
a
s
_
d
e
r
e
f
(
)
,
 
l
i
n
e
s
,
 
w
o
r
d
s
,
 
c
h
a
r
s
,
 
b
y
t
e
s
,
 
m
a
x
_
l
i
n
e
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
B
u
i
l
d
 
{
 
d
i
r
,
 
t
a
r
g
e
t
,
 
p
r
o
f
i
l
e
,
 
b
i
n
,
 
f
e
a
t
u
r
e
s
,
 
w
o
r
k
s
p
a
c
e
,
 
l
i
s
t
_
t
a
r
g
e
t
s
,
 
n
o
_
c
o
n
f
i
g
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
b
u
i
l
d
:
:
r
u
n
(
d
i
r
.
a
s
_
d
e
r
e
f
(
)
,
 
t
a
r
g
e
t
.
a
s
_
d
e
r
e
f
(
)
,
 
p
r
o
f
i
l
e
.
a
s
_
d
e
r
e
f
(
)
,
 
b
i
n
.
a
s
_
d
e
r
e
f
(
)
,
 
f
e
a
t
u
r
e
s
,
 
w
o
r
k
s
p
a
c
e
,
 
l
i
s
t
_
t
a
r
g
e
t
s
,
 
n
o
_
c
o
n
f
i
g
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
C
h
e
c
k
 
{
 
d
i
r
,
 
c
l
i
p
p
y
,
 
f
m
t
,
 
f
i
x
 
}
 
=
>
 
c
h
e
c
k
:
:
r
u
n
(
d
i
r
.
a
s
_
d
e
r
e
f
(
)
,
 
c
l
i
p
p
y
,
 
f
m
t
,
 
f
i
x
)
?
,


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
S
i
z
e
 
{
 
d
i
r
,
 
t
a
r
g
e
t
,
 
p
r
o
f
i
l
e
,
 
a
l
l
,
 
h
u
m
a
n
,
 
s
o
r
t
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
s
i
z
e
:
:
r
u
n
(
d
i
r
.
a
s
_
d
e
r
e
f
(
)
,
 
t
a
r
g
e
t
.
a
s
_
d
e
r
e
f
(
)
,
 
p
r
o
f
i
l
e
.
a
s
_
d
e
r
e
f
(
)
,
 
a
l
l
,
 
h
u
m
a
n
,
 
s
o
r
t
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
C
l
e
a
n
 
{
 
d
i
r
,
 
t
a
r
g
e
t
,
 
p
r
o
f
i
l
e
,
 
d
r
y
_
r
u
n
,
 
a
l
l
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
c
l
e
a
n
:
:
r
u
n
(
d
i
r
.
a
s
_
d
e
r
e
f
(
)
,
 
t
a
r
g
e
t
.
a
s
_
d
e
r
e
f
(
)
,
 
p
r
o
f
i
l
e
.
a
s
_
d
e
r
e
f
(
)
,
 
d
r
y
_
r
u
n
,
 
a
l
l
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
 
 
 
 
C
o
m
m
a
n
d
:
:
N
o
r
m
a
l
i
z
e
 
{
 
p
a
t
h
,
 
e
n
d
i
n
g
,
 
r
e
m
o
v
e
_
b
o
m
,
 
j
s
o
n
 
}
 
=
>
 
{


 
 
 
 
 
 
 
 
 
 
 
 
n
o
r
m
a
l
i
z
e
:
:
r
u
n
(
&
p
a
t
h
,
 
e
n
d
i
n
g
.
a
s
_
d
e
r
e
f
(
)
,
 
r
e
m
o
v
e
_
b
o
m
,
 
j
s
o
n
)
?
;


 
 
 
 
 
 
 
 
}


 
 
 
 
}


 
 
 
 
O
k
(
(
)
)


}


'
'
'




w
i
t
h
 
o
p
e
n
(
'
/
h
o
m
e
/
h
u
h
u
/
p
r
o
j
e
c
t
s
/
r
x
t
/
s
r
c
/
m
a
i
n
.
r
s
'
,
 
'
w
'
,
 
e
n
c
o
d
i
n
g
=
'
u
t
f
-
8
'
,
 
n
e
w
l
i
n
e
=
'
\
n
'
)
 
a
s
 
f
:


 
 
 
 
f
.
w
r
i
t
e
(
c
o
n
t
e
n
t
)




p
r
i
n
t
(
f
"
O
K
:
 
w
r
o
t
e
 
{
l
e
n
(
c
o
n
t
e
n
t
)
}
 
c
h
a
r
s
"
)

