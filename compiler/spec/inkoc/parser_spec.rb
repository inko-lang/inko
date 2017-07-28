# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Parser do
  def parse(source)
    Inkoc::Parser.new(source).parse
  end

  %w[|| && == != < <= > >= | ^ & << >> + - / % * ** .. ...].each do |op|
    describe "binary #{op}" do
      let(:node) { parse("10 #{op} 20").expressions[0] }

      it 'is parsed as a message send' do
        expect(node).to be_an_instance_of(Inkoc::AST::Send)
      end

      it 'sets the message name to the operator' do
        expect(node.name).to eq(op)
      end

      it 'sets the receiver to the left-hand side value' do
        expect(node.receiver).to be_an_instance_of(Inkoc::AST::Integer)
        expect(node.receiver.value).to eq(10)
      end

      it 'passes the right-hand side as an argument' do
        arg = node.arguments[0]

        expect(arg).to be_an_instance_of(Inkoc::AST::Integer)
        expect(arg.value).to eq(20)
      end

      it 'sets the source location of the operation' do
        expect(node.location.line).to eq(1)
        expect(node.location.column).to eq(4)
      end

      it 'sets the source location of the receiver' do
        expect(node.receiver.location.line).to eq(1)
        expect(node.receiver.location.column).to eq(1)
      end

      it 'sets the source location of the argument' do
        arg = node.arguments[0]

        expect(arg.location.line).to eq(1)
        expect(arg.location.column).to eq(op.length + 5)
      end
    end
  end

  describe 'reading slice indexes' do
    let(:node) { parse('10[20]').expressions[0] }

    it 'is parsed as a message send' do
      expect(node).to be_an_instance_of(Inkoc::AST::Send)
    end

    it 'sets the message name to []' do
      expect(node.name).to eq('[]')
    end

    it 'sets the receiver of the value being sliced' do
      expect(node.receiver).to be_an_instance_of(Inkoc::AST::Integer)
      expect(node.receiver.value).to eq(10)
    end

    it 'passes the slice index as an argument' do
      arg = node.arguments[0]

      expect(arg).to be_an_instance_of(Inkoc::AST::Integer)
      expect(arg.value).to eq(20)
    end
  end

  describe 'setting slice indexes' do
    let(:node) { parse('10[20] = 30').expressions[0] }

    it 'is parsed as a message send' do
      expect(node).to be_an_instance_of(Inkoc::AST::Send)
    end

    it 'sets the message name to []' do
      expect(node.name).to eq('[]=')
    end

    it 'sets the receiver of the value being sliced' do
      expect(node.receiver).to be_an_instance_of(Inkoc::AST::Integer)
      expect(node.receiver.value).to eq(10)
    end

    it 'passes the slice index as an argument' do
      arg = node.arguments[0]

      expect(arg).to be_an_instance_of(Inkoc::AST::Integer)
      expect(arg.value).to eq(20)
    end

    it 'passes the value to set as an argument' do
      arg = node.arguments[1]

      expect(arg).to be_an_instance_of(Inkoc::AST::Integer)
      expect(arg.value).to eq(30)
    end
  end

  describe 'type casting' do
    let(:node) { parse('10 as Foo').expressions[0] }

    it 'is parsed as a TypeCast' do
      expect(node).to be_an_instance_of(Inkoc::AST::TypeCast)
    end

    it 'sets the expression to cast' do
      expect(node.expression).to be_an_instance_of(Inkoc::AST::Integer)
    end

    it 'sets the type to cast to' do
      expect(node.cast_to).to be_an_instance_of(Inkoc::AST::Constant)
    end
  end

  describe 'send chains' do
    let(:node) { parse('foo.bar.baz').expressions[0] }

    it 'is parses as a Send' do
      expect(node).to be_an_instance_of(Inkoc::AST::Send)
      expect(node.name).to eq('baz')
    end

    it 'sets the receivers' do
      expect(node.receiver.name).to eq('bar')
      expect(node.receiver.receiver.name).to eq('foo')
    end
  end

  describe 'send chains with arguments' do
    let(:node) { parse('foo(10).bar(20).baz(30)').expressions[0] }

    it 'sets the arguments for every message send' do
      expect(node.arguments[0].value).to eq(30)
      expect(node.receiver.arguments[0].value).to eq(20)
      expect(node.receiver.receiver.arguments[0].value).to eq(10)
    end
  end

  describe 'message sends without explicit parenthesis' do
    let(:body) { parse("foo 10, 20\n30") }
    let(:send_node) { body.expressions[0] }

    it 'parses expressions on the same line as arguments' do
      expect(send_node.arguments[0].value).to eq(10)
      expect(send_node.arguments[1].value).to eq(20)
    end

    it 'parses expressions on the next line separately' do
      expect(body.expressions[1].value).to eq(30)
    end
  end
end
