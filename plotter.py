# ANT+ Heart Rate Monitor Plotter
#
# Permission is hereby granted, free of charge, to any person obtaining a
# copy of this software and associated documentation files (the "Software"),
# to deal in the Software without restriction, including without limitation
# the rights to use, copy, modify, merge, publish, distribute, sublicense,
# and/or sell copies of the Software, and to permit persons to whom the
# Software is furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in
# all copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
# FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
# DEALINGS IN THE SOFTWARE.

import tk
import os
import signal
import sys
import threading
import time
import configparser

import matplotlib
import matplotlib.pyplot as plt
matplotlib.use('TkAgg')
from matplotlib.animation import FuncAnimation
import numpy as np

from ant.easy.node import Node
from ant.easy.channel import Channel
from ant.base.message import Message


NETWORK_KEY = [0xB9, 0xA5, 0x21, 0xFB, 0xBD, 0x72, 0xC3, 0x45]


def on_data(rawdata):
    heartrate = rawdata[7]
    string = "Heartrate: " + str(heartrate) + " [BPM]"
    data.hr = heartrate
    sys.stdout.write(string)
    sys.stdout.flush()
    sys.stdout.write("\b" * len(string))

class allData():
    def __init__(self):
        #self.X=np.linspace(0,xpts-1,xpts)
        #self.Y=np.linspace(0,0,xpts)
        self.X=np.array([0])
        self.Y=np.array([0])
        self.hr = 0


class plotHeartrate():
    def __init__(self, data):
        self._data = data
        plt.rcParams['toolbar']='None'
        plt.xlabel('Time (s)')
        plt.ylabel('Heartrate (BPM)')
        
        self.hLine, = plt.plot(0, 0, color=linecolor)
        
        ax = self.hLine.axes
        ax.set_facecolor(backgroundcolor)
        ax.xaxis.label.set_color(labelscolor)
        ax.yaxis.label.set_color(labelscolor)
        for s in ['left', 'right', 'top', 'bottom']:
            ax.spines[s].set_color(spinescolor)
        ax.tick_params(axis='x', colors=spinescolor)
        ax.tick_params(axis='y', colors=spinescolor)
        
        self.figure = plt.gcf()
        self.figure.set_size_inches((width, height))
        self.figure.set_facecolor(backgroundcolor)
        self.figure.tight_layout()
        self._hrText = self.figure.text(0,0.5,'hi',fontsize=30,color=labelscolor)
        plt.subplots_adjust(left=0.25)
        self.figure.canvas.manager.window.wm_geometry("+"+str(posx)+"+"+str(posy))
        self.ani = FuncAnimation(plt.gcf(), self.run, interval = intervalms, repeat=True)

    def run(self, i): 
        if (self._data.X.size == self._data.Y.size): 
            self.hLine.set_data(self._data.X, self._data.Y)
            plt.ylim([ybottom, ytop])        
            self.hLine.axes.relim()
            self.hLine.axes.autoscale_view(False, 'both')
            self.hLine.axes.autoscale_view(True, 'x')
            self._hrText.set_text(str(self._data.Y[-1]))
            #self.hLine.ylim([0, 200])
            #plt.xlim([0, 100])
        

class dataFetch(threading.Thread):

    def __init__(self, runFlag, data):
        super(dataFetch, self).__init__()
        self._runFlag = runFlag
        self._data = data
        self._period = intervalms/1000
        self._lastCall = time.time()
        self._start = time.time()
        
    def run(self):
        print("starting fetcher")
        while not self._runFlag.is_set():
            # add data       
            if self._data.X.size < xpts:
                # print("load")
                self._data.X = np.append(self._data.X, (time.time() - self._start))
                self._data.Y = np.append(self._data.Y, (self._data.hr))
            else:
                self._data.Y=np.roll(self._data.Y,-1)
                self._data.Y[-1]=self._data.hr
                self._data.X=np.roll(self._data.X,-1)
                self._data.X[-1]=self._lastCall
            # print(self._data.X)
            # print(self._data.Y)
            # sleep until next execution
            self._lastCall = time.time() - self._start
            time.sleep(self._period)
        print("ending fetcher")


class antDataGenerator(threading.Thread):
    def __init__(self, runFlag, node, channel):
        super(antDataGenerator, self).__init__()
        self._runFlag = runFlag
        self._node = node
        self._channel = channel

    def run(self):        
        while not self._runFlag.is_set():
            print('opening channel')
            try:
                self._channel.open()
                self._node.start()
            finally:
                print('channel no longer open')
        print("ending generator")



cfg = configparser.ConfigParser()

if not os.path.exists('settings.ini'):
    print('Could not find settings.ini file, creating one')
    cfg['plotSettings']={
    'backgroundcolor': 'black',
    'linecolor': 'cyan',
    'labelscolor': 'cyan',
    'spinescolor': 'whitesmoke',
    'xpts': '50',
    'intervalms': '500',
    'ybottom': '0',
    'ytop': '200',
    'width': '6',
    'height': '4',
    'posx': '0',
    'posy': '0'
    }
    cfg.write(open('settings.ini','w'))

cfg.read('settings.ini')

#parameters
backgroundcolor=cfg.get('plotSettings', 'backgroundcolor')
linecolor=cfg.get('plotSettings', 'linecolor')
labelscolor=cfg.get('plotSettings', 'labelscolor')
spinescolor=cfg.get('plotSettings', 'spinescolor')
xpts=cfg.getint('plotSettings', 'xpts')
intervalms=cfg.getint('plotSettings', 'intervalms')
ybottom=cfg.getint('plotSettings', 'ybottom')
ytop=cfg.getint('plotSettings', 'ytop')
width=cfg.getint('plotSettings', 'width')
height=cfg.getint('plotSettings', 'height')
posx=cfg.getint('plotSettings', 'posx')
posy=cfg.getint('plotSettings', 'posy')


#initiate data class
data = allData()

def main():
    node = Node()
    node.set_network_key(0x00, NETWORK_KEY)
    channel = node.new_channel(Channel.Type.BIDIRECTIONAL_RECEIVE)
    #self.channel.on_broadcast_data = self.on_data
    channel.on_broadcast_data = on_data
    channel.set_period(8070)
    channel.set_search_timeout(12)
    channel.set_rf_freq(57)
    channel.set_id(0, 120, 0)


    runFlag = threading.Event()
    fetcher = dataFetch(runFlag, data)
    gen = antDataGenerator(runFlag, node, channel)
    #start the plotter and the data fetcher
    plotter = plotHeartrate(data)
    fetcher.start()
    gen.start()
   
    try:
        plt.show()
        #show the plot        
    finally:
        runFlag.set()
        channel.close()
        node.stop()
        print("channel closed")
        fetcher.join()
        gen.join()
        print("threads successfully closed")

if __name__ == '__main__':
    main()
