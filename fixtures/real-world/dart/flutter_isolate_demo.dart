// Copyright 2019-present the Flutter authors. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");

import 'dart:async';
import 'dart:io' show Platform;
import 'dart:isolate';
import 'dart:math';

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:window_size/window_size.dart';

void main() {
  setupWindow();
  runApp(const IsolateExampleApp());
}

const double windowWidth = 1024;
const double windowHeight = 800;

void setupWindow() {
  if (!kIsWeb &&
      (Platform.isWindows || Platform.isLinux || Platform.isMacOS)) {
    WidgetsFlutterBinding.ensureInitialized();
    setWindowTitle('Isolate Example');
    setWindowMinSize(const Size(windowWidth, windowHeight));
  }
}

class IsolateExampleApp extends StatelessWidget {
  const IsolateExampleApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Isolate Demo',
      theme: ThemeData(
        primarySwatch: Colors.blue,
        visualDensity: VisualDensity.adaptivePlatformDensity,
      ),
      home: const HomePage(),
    );
  }
}

class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> with TickerProviderStateMixin {
  late TabController _tabController;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 3, vsync: this);
  }

  @override
  void dispose() {
    _tabController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Isolate Examples'),
        bottom: TabBar(
          controller: _tabController,
          tabs: const [
            Tab(icon: Icon(Icons.flash_on), text: 'Performance'),
            Tab(icon: Icon(Icons.sync), text: 'Infinite Process'),
            Tab(icon: Icon(Icons.swap_horiz), text: 'Data Transfer'),
          ],
        ),
      ),
      body: TabBarView(
        controller: _tabController,
        children: const [
          PerformancePage(),
          InfiniteProcessPage(),
          DataTransferPage(),
        ],
      ),
    );
  }
}

// Performance comparison page
class PerformancePage extends StatefulWidget {
  const PerformancePage({super.key});

  @override
  State<PerformancePage> createState() => _PerformancePageState();
}

class _PerformancePageState extends State<PerformancePage> {
  bool _isCalculatingMain = false;
  bool _isCalculatingIsolate = false;
  String _mainThreadResult = '';
  String _isolateResult = '';

  Future<void> _calculateOnMainThread() async {
    setState(() {
      _isCalculatingMain = true;
      _mainThreadResult = '';
    });

    final stopwatch = Stopwatch()..start();
    final result = _heavyComputation(1000000);
    stopwatch.stop();

    setState(() {
      _isCalculatingMain = false;
      _mainThreadResult = 'Result: $result\nTime: ${stopwatch.elapsedMilliseconds}ms';
    });
  }

  Future<void> _calculateOnIsolate() async {
    setState(() {
      _isCalculatingIsolate = true;
      _isolateResult = '';
    });

    final stopwatch = Stopwatch()..start();
    final result = await _heavyComputationInIsolate(1000000);
    stopwatch.stop();

    setState(() {
      _isCalculatingIsolate = false;
      _isolateResult = 'Result: $result\nTime: ${stopwatch.elapsedMilliseconds}ms';
    });
  }

  static int _heavyComputation(int value) {
    var result = 0;
    for (var i = 0; i < value; i++) {
      result += i * 2;
    }
    return result;
  }

  static Future<int> _heavyComputationInIsolate(int value) async {
    final receivePort = ReceivePort();

    await Isolate.spawn<Map<String, dynamic>>(
      _isolateEntryPoint,
      {
        'sendPort': receivePort.sendPort,
        'value': value,
      },
    );

    return await receivePort.first as int;
  }

  static void _isolateEntryPoint(Map<String, dynamic> params) {
    final sendPort = params['sendPort'] as SendPort;
    final value = params['value'] as int;

    final result = _heavyComputation(value);
    sendPort.send(result);
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text(
            'Performance Comparison',
            style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 20),

          // Main thread section
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text('Main Thread Computation',
                      style: TextStyle(fontSize: 18)),
                  const SizedBox(height: 10),
                  ElevatedButton(
                    onPressed: _isCalculatingMain ? null : _calculateOnMainThread,
                    child: _isCalculatingMain
                        ? const CircularProgressIndicator()
                        : const Text('Calculate on Main Thread'),
                  ),
                  const SizedBox(height: 10),
                  Text(_mainThreadResult),
                ],
              ),
            ),
          ),

          const SizedBox(height: 16),

          // Isolate section
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text('Isolate Computation',
                      style: TextStyle(fontSize: 18)),
                  const SizedBox(height: 10),
                  ElevatedButton(
                    onPressed: _isCalculatingIsolate ? null : _calculateOnIsolate,
                    child: _isCalculatingIsolate
                        ? const CircularProgressIndicator()
                        : const Text('Calculate in Isolate'),
                  ),
                  const SizedBox(height: 10),
                  Text(_isolateResult),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

// Infinite process page
class InfiniteProcessPage extends StatefulWidget {
  const InfiniteProcessPage({super.key});

  @override
  State<InfiniteProcessPage> createState() => _InfiniteProcessPageState();
}

class _InfiniteProcessPageState extends State<InfiniteProcessPage> {
  bool _isRunning = false;
  int _counter = 0;
  Isolate? _isolate;
  late ReceivePort _receivePort;

  @override
  void initState() {
    super.initState();
    _receivePort = ReceivePort();
    _receivePort.listen((message) {
      setState(() {
        _counter = message as int;
      });
    });
  }

  @override
  void dispose() {
    _receivePort.close();
    _isolate?.kill();
    super.dispose();
  }

  void _startInfiniteProcess() async {
    setState(() {
      _isRunning = true;
      _counter = 0;
    });

    _isolate = await Isolate.spawn(
      _infiniteLoopEntryPoint,
      _receivePort.sendPort,
    );
  }

  void _stopInfiniteProcess() {
    _isolate?.kill();
    setState(() {
      _isRunning = false;
    });
  }

  static void _infiniteLoopEntryPoint(SendPort sendPort) {
    var counter = 0;
    Timer.periodic(const Duration(milliseconds: 100), (timer) {
      counter++;
      sendPort.send(counter);
    });
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const Text(
            'Infinite Process Demo',
            style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 20),

          Card(
            child: Padding(
              padding: const EdgeInsets.all(32.0),
              child: Column(
                children: [
                  Text(
                    'Counter: $_counter',
                    style: const TextStyle(fontSize: 36),
                  ),
                  const SizedBox(height: 20),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                    children: [
                      ElevatedButton(
                        onPressed: _isRunning ? null : _startInfiniteProcess,
                        child: const Text('Start'),
                      ),
                      ElevatedButton(
                        onPressed: _isRunning ? _stopInfiniteProcess : null,
                        child: const Text('Stop'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

// Data transfer page
class DataTransferPage extends StatefulWidget {
  const DataTransferPage({super.key});

  @override
  State<DataTransferPage> createState() => _DataTransferPageState();
}

class _DataTransferPageState extends State<DataTransferPage> {
  final TextEditingController _controller = TextEditingController();
  String _processedText = '';
  bool _isProcessing = false;

  Future<void> _processText() async {
    final inputText = _controller.text;
    if (inputText.isEmpty) return;

    setState(() {
      _isProcessing = true;
      _processedText = '';
    });

    try {
      final result = await _processTextInIsolate(inputText);
      setState(() {
        _processedText = result;
      });
    } finally {
      setState(() {
        _isProcessing = false;
      });
    }
  }

  static Future<String> _processTextInIsolate(String text) async {
    final receivePort = ReceivePort();

    await Isolate.spawn<Map<String, dynamic>>(
      _textProcessingEntryPoint,
      {
        'sendPort': receivePort.sendPort,
        'text': text,
      },
    );

    return await receivePort.first as String;
  }

  static void _textProcessingEntryPoint(Map<String, dynamic> params) {
    final sendPort = params['sendPort'] as SendPort;
    final text = params['text'] as String;

    // Simulate complex text processing
    final words = text.split(' ');
    final processedWords = words.map((word) {
      return word.split('').reversed.join('');
    }).toList();

    // Add some artificial delay
    for (var i = 0; i < 1000000; i++) {
      // Busy work
    }

    final result = processedWords.join(' ');
    sendPort.send(result);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text(
            'Data Transfer Demo',
            style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold),
          ),
          const SizedBox(height: 20),

          TextField(
            controller: _controller,
            maxLines: 3,
            decoration: const InputDecoration(
              labelText: 'Enter text to process',
              border: OutlineInputBorder(),
            ),
          ),

          const SizedBox(height: 16),

          ElevatedButton(
            onPressed: _isProcessing ? null : _processText,
            child: _isProcessing
                ? const CircularProgressIndicator()
                : const Text('Process Text in Isolate'),
          ),

          const SizedBox(height: 16),

          Card(
            child: Padding(
              padding: const EdgeInsets.all(16.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    'Processed Result:',
                    style: TextStyle(fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 8),
                  Text(_processedText),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}