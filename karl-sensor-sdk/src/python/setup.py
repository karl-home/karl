from setuptools import setup

setup(name='karl-sensor-sdk',
      version='0.1.1',
      description='SDK for Karl sensor code',
      url='http://github.com/karl-home/karl',
      author='Devrath Iyer',
      license='MIT',
	  install_requires=[
		'grpcio-tools'
      ],
      packages=['karl'],
      zip_safe=False)
