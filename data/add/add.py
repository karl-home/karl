import sys
if len(sys.argv) <= 1:
    print('USAGE: python run.py [N_ITERATIONS]')
else:
    n = 1 << int(sys.argv[1])
    counter = 0
    for i in range(n):
        counter += i
    with open('output.txt', 'w+') as f:
        print(counter)
        f.write(str(counter))
