# frozen_string_literal: true

require 'spec_helper'

describe Inkoc::Type::Block do
  let(:block1) { described_class.new }
  let(:block2) { described_class.new }

  describe '#implemented_traits' do
    describe 'without a prototype' do
      it 'returns an empty Set' do
        expect(block1.implemented_traits).to eq(Set.new)
      end
    end

    describe 'with a prototype' do
      it 'returns the implemented traits of the prototype' do
        proto = Inkoc::Type::Object.new
        trait = Inkoc::Type::Trait.new
        block = described_class.new(prototype: proto)

        proto.implemented_traits << trait

        expect(block.implemented_traits).to eq(Set.new([trait]))
      end
    end
  end

  describe '#infer?' do
    describe 'with a closure' do
      let(:block) { described_class.new(block_type: :closure) }

      it 'returns true when the arguments are not inferred' do
        expect(block.infer?).to eq(true)
      end

      it 'returns false when the arguments are inferred' do
        block.inferred = true

        expect(block.infer?).to eq(false)
      end
    end

    describe 'with a method' do
      it 'returns false' do
        block = described_class.new(block_type: :method)

        expect(block.infer?).to eq(false)
      end
    end
  end

  describe '#closure?' do
    it 'returns true for a closure' do
      expect(block1.closure?).to eq(true)
    end

    it 'returns false for a method' do
      block = described_class.new(block_type: :method)

      expect(block.closure?).to eq(false)
    end
  end

  describe '#method?' do
    it 'returns true for a method' do
      block = described_class.new(block_type: :method)

      expect(block.method?).to eq(true)
    end

    it 'returns false for a closure' do
      expect(block1.method?).to eq(false)
    end
  end

  describe '#valid_number_of_arguments?' do
    before do
      block1.define_self_argument(Inkoc::Type::Object.new)
    end

    it 'returns false when not enough arguments are given' do
      block1.define_required_argument('name', Inkoc::Type::Object.new)

      expect(block1.valid_number_of_arguments?(0)).to eq(false)
    end

    it 'returns true when enough arguments are given' do
      block1.define_required_argument('name', Inkoc::Type::Object.new)

      expect(block1.valid_number_of_arguments?(1)).to eq(true)
    end

    describe 'when a rest argument is not defined' do
      it 'returns false when too many arguments are given' do
        block1.define_required_argument('name', Inkoc::Type::Object.new)

        expect(block1.valid_number_of_arguments?(2)).to eq(false)
      end
    end

    describe 'when a rest argument is defined' do
      it 'returns true when too many arguments are given' do
        block1.define_rest_argument('names', Inkoc::Type::Object.new)

        expect(block1.valid_number_of_arguments?(5)).to eq(true)
      end
    end
  end

  describe '#arguments_count' do
    it 'returns the number of arguments' do
      block1.define_argument('name', Inkoc::Type::Object.new)

      expect(block1.arguments_count).to eq(1)
    end
  end

  describe '#required_arguments_count_without_self' do
    it 'returns the number of required arguments, excluding self' do
      block1.define_self_argument(Inkoc::Type::Object.new)
      block1.define_required_argument('name', Inkoc::Type::Object.new)

      expect(block1.required_arguments_count_without_self).to eq(1)
    end
  end

  describe '#arguments_count_without_self' do
    it 'returns the number of arguments, excluding self' do
      block1.define_self_argument(Inkoc::Type::Object.new)
      block1.define_argument('name', Inkoc::Type::Object.new)

      expect(block1.arguments_count_without_self).to eq(1)
    end
  end

  describe '#argument_count_range' do
    it 'returns a range covering the number of arguments' do
      block1.define_self_argument(Inkoc::Type::Object.new)
      block1.define_required_argument('name', Inkoc::Type::Object.new)
      block1.define_argument('number', Inkoc::Type::Object.new)

      expect(block1.argument_count_range).to eq(1..2)
    end
  end

  describe '#define_self_argument' do
    it 'defines the argument for "self"' do
      type = Inkoc::Type::Object.new

      block1.define_self_argument(type)

      expect(block1.arguments['self'].type).to eq(type)
    end
  end

  describe '#define_required_argument' do
    it 'defines an immutable required argument' do
      block1.define_required_argument('name', Inkoc::Type::Object.new)

      expect(block1.arguments['name'].mutable?).to eq(false)
      expect(block1.required_arguments_count).to eq(1)
    end

    it 'defines a mutable required argument' do
      block1.define_required_argument('name', Inkoc::Type::Object.new, true)

      expect(block1.arguments['name'].mutable?).to eq(true)
      expect(block1.required_arguments_count).to eq(1)
    end
  end

  describe '#define_argument' do
    it 'defines an immutable argument' do
      block1.define_argument('name', Inkoc::Type::Object.new)

      expect(block1.arguments['name'].mutable?).to eq(false)
      expect(block1.arguments_count).to eq(1)
    end

    it 'defines a mutable argument' do
      block1.define_argument('name', Inkoc::Type::Object.new, true)

      expect(block1.arguments['name'].mutable?).to eq(true)
      expect(block1.arguments_count).to eq(1)
    end
  end

  describe '#define_rest_argument' do
    it 'defines an immutable rest argument' do
      block1.define_rest_argument('name', Inkoc::Type::Object.new)

      expect(block1.arguments['name'].mutable?).to eq(false)
      expect(block1.rest_argument).to eq(true)
    end

    it 'defines a mutable rest argument' do
      block1.define_rest_argument('name', Inkoc::Type::Object.new, true)

      expect(block1.arguments['name'].mutable?).to eq(true)
      expect(block1.rest_argument).to eq(true)
    end
  end

  describe '#block?' do
    it 'returns true' do
      expect(block1.block?).to eq(true)
    end
  end

  describe '#return_type' do
    it 'returns the return type' do
      type = Inkoc::Type::Object.new
      block1.returns = type

      expect(block1.return_type).to eq(type)
    end
  end

  describe '#define_type_parameter' do
    it 'defines a type parameter' do
      type = Inkoc::Type::Object.new

      block1.define_type_parameter('T', type)

      expect(block1.type_parameters['T']).to eq(type)
    end
  end

  describe '#lookup_argument' do
    it 'returns a Symbol for an existing argument' do
      type = Inkoc::Type::Object.new

      block1.define_argument('name', type)

      symbol = block1.lookup_argument('name')

      expect(symbol).to be_an_instance_of(Inkoc::Symbol)
      expect(symbol.type).to eq(type)
    end

    it 'returns a NullSymbol for a non-existing argument' do
      symbol = block1.lookup_argument('name')

      expect(symbol).to be_an_instance_of(Inkoc::NullSymbol)
    end
  end

  describe '#type_for_argument_or_rest' do
    describe 'when using the name of an existing argument' do
      it 'returns the type of the argument' do
        type = Inkoc::Type::Object.new

        block1.define_argument('name', type)

        expect(block1.type_for_argument_or_rest('name')).to eq(type)
      end
    end

    describe 'when using the name of a non-existing argument' do
      it 'returns the type of the last defined argument' do
        type = Inkoc::Type::Object.new

        block1.define_rest_argument('rest', type)

        expect(block1.type_for_argument_or_rest('name')).to eq(type)
      end
    end
  end

  describe '#initialized_return_type' do
    let(:self_type) { Inkoc::Type::Object.new(name: 'A') }

    before do
      block1.define_self_argument(self_type)
    end

    describe 'without passing any argument types' do
      it 'returns the initialized return type' do
        return_type = Inkoc::Type::Object.new(name: 'B')
        block1.returns = return_type
        init_type = block1.initialized_return_type(self_type)

        expect(init_type.prototype).to eq(return_type)
      end
    end

    describe 'when returning a Self type' do
      it 'resolves the Self type to the actual type of "self"' do
        block1.returns = Inkoc::Type::SelfType.new
        init_type = block1.initialized_return_type(self_type)

        expect(init_type.prototype).to eq(self_type)
      end
    end

    describe 'when the block defines a type parameter' do
      it 'initializes the type parameter in the returned type' do
        param = Inkoc::Type::Trait.new(name: 'T', generated: true)
        concrete = Inkoc::Type::Object.new(name: 'Integer')

        block1.define_type_parameter('T', param)
        block1.define_argument('number', param)
        block1.returns = Inkoc::Type::Object.new

        init_type = block1.initialized_return_type(self_type, [concrete])

        expect(init_type.lookup_type('T')).to eq(concrete)
      end
    end

    describe 'when returning a type parameter' do
      it 'initializes the type parameter using a concrete type' do
        param = Inkoc::Type::Trait.new(name: 'T', generated: true)
        concrete = Inkoc::Type::Object.new(name: 'Integer')

        block1.define_type_parameter('T', param)
        block1.define_argument('number', param)
        block1.returns = param

        init_type = block1.initialized_return_type(self_type, [concrete])

        expect(init_type.prototype).to eq(concrete)
      end
    end
  end

  describe '#lookup_type' do
    let(:type) { Inkoc::Type::Object.new }
    let(:param) { Inkoc::Type::Trait.new(generated: true) }

    before do
      block1.attributes.define('name', type)
      block1.define_type_parameter('T', param)
    end

    describe 'using the name of a defined attribute' do
      it 'returns the type of the attribute' do
        expect(block1.lookup_type('name')).to eq(type)
      end
    end

    describe 'using the name of a type parameter' do
      it 'returns the type of the type parameter' do
        expect(block1.lookup_type('T')).to eq(param)
      end
    end

    describe 'using the name of an undefined symbol' do
      it 'returns nil' do
        expect(block1.lookup_type('foo')).to be_nil
      end
    end
  end

  describe '#implementation_of?' do
    describe 'when the block is not an implementation of another block' do
      it 'returns false' do
        block2.returns = Inkoc::Type::Object.new

        expect(block1.implementation_of?(block2)).to eq(false)
      end
    end

    describe 'when the block is an implementation of another block' do
      it 'returns true' do
        block2.returns = Inkoc::Type::Trait.new(name: 'A')
        block1.returns = Inkoc::Type::Object.new(name: 'B')

        block1.returns.implemented_traits << block2.returns

        expect(block1.implementation_of?(block2)).to eq(true)
      end
    end

    describe 'when the blocks are compatible but their names differ' do
      it 'returns false' do
        block1 = described_class.new(name: 'foo')
        block2 = described_class.new(name: 'bar')

        block2.returns = Inkoc::Type::Trait.new(name: 'A')
        block1.returns = Inkoc::Type::Object.new(name: 'B')

        block1.returns.implemented_traits << block2.returns

        expect(block1.implementation_of?(block2)).to eq(false)
      end
    end
  end

  describe '#type_parameter_values' do
    it 'returns the defined type parameters' do
      param = Inkoc::Type::Trait.new(generated: true)

      block1.define_type_parameter('T', param)

      expect(block1.type_parameter_values).to eq([param])
    end
  end

  describe '#type_parameters_compatible?' do
    it 'returns false when the number of type parameters is not the same' do
      tparam = Inkoc::Type::Trait.new(generated: true)

      block1.define_type_parameter('foo', tparam)

      expect(block1.type_parameters_compatible?(block2)).to eq(false)
    end

    it 'returns false if the type parameters are not compatible' do
      param1 = Inkoc::Type::Trait.new(name: 'T', generated: true)
      param2 = Inkoc::Type::Trait.new(name: 'T', generated: true)
      method = described_class
        .new(name: 'foo', returns: Inkoc::Type::Object.new)

      param1.define_required_method(method)

      block1.define_type_parameter(param1.name, param1)
      block2.define_type_parameter(param2.name, param2)

      expect(block1.type_parameters_compatible?(block2)).to eq(false)
    end

    it 'returns true if the type parameters are compatible' do
      param1 = Inkoc::Type::Trait.new(generated: true)
      param2 = Inkoc::Type::Trait.new(generated: true)
      method = described_class
        .new(name: 'foo', returns: Inkoc::Type::Object.new)

      param1.define_required_method(method)
      param2.define_required_method(method)

      block1.define_type_parameter(param1.name, param1)
      block2.define_type_parameter(param2.name, param2)

      expect(block1.type_parameters_compatible?(block2)).to eq(true)
    end
  end

  describe '#argument_types_compatible?' do
    it 'returns false when the number of arguments is not the same' do
      block1.define_argument('number', Inkoc::Type::Object.new)

      expect(block1.argument_types_compatible?(block2)).to eq(false)
    end

    it 'returns false if the arguments are not compatible' do
      block1.define_argument('number', Inkoc::Type::Object.new)
      block2.define_argument('number', Inkoc::Type::Trait.new)

      expect(block1.argument_types_compatible?(block2)).to eq(false)
    end

    it 'returns true if the arguments are compatible' do
      parent = Inkoc::Type::Object.new
      child = Inkoc::Type::Object.new(prototype: parent)

      block1.define_argument('number', child)
      block2.define_argument('number', parent)

      expect(block1.argument_types_compatible?(block2)).to eq(true)
    end
  end

  describe '#throw_types_compatible?' do
    describe 'when the source and target blocks throw a value' do
      it 'returns true when the thrown value is compatible' do
        trait = Inkoc::Type::Trait.new
        object = Inkoc::Type::Object.new

        object.implemented_traits << trait

        block1.throws = object
        block2.throws = trait

        expect(block1.throw_types_compatible?(block2)).to eq(true)
      end

      it 'returns false when the thrown value is not compatible' do
        trait = Inkoc::Type::Trait.new
        object = Inkoc::Type::Object.new

        block1.throws = object
        block2.throws = trait

        expect(block1.throw_types_compatible?(block2)).to eq(false)
      end
    end

    describe 'when the source block throws but the target block does not' do
      it 'returns true if the source block is a closure' do
        block1.throws = Inkoc::Type::Object.new

        expect(block1.throw_types_compatible?(block2)).to eq(true)
      end

      it 'returns false if the source block is not a closure' do
        block1 = described_class.new(block_type: :method)
        block1.throws = Inkoc::Type::Object.new

        expect(block1.throw_types_compatible?(block2)).to eq(false)
      end
    end

    describe 'when the source block does not throw a value' do
      it 'returns true if the other block does not throw a value' do
        expect(block1.throw_types_compatible?(block2)).to eq(true)
      end

      it 'returns true if the other block throws a value' do
        block2.throws = Inkoc::Type::Object.new

        expect(block1.throw_types_compatible?(block2)).to eq(true)
      end
    end
  end

  describe '#return_types_compatible?' do
    it 'returns true when the types are compatible' do
      parent = Inkoc::Type::Object.new
      child = Inkoc::Type::Object.new(prototype: parent)

      block1.returns = child
      block2.returns = parent

      expect(block1.return_types_compatible?(block2)).to eq(true)
    end

    it 'returns false when the types are not compatible' do
      block1.returns = Inkoc::Type::Object.new
      block2.returns = Inkoc::Type::Object.new

      expect(block1.return_types_compatible?(block2)).to eq(false)
    end
  end

  describe '#type_compatible?' do
    it 'returns true when compared with itself' do
      expect(block1.type_compatible?(block1)).to eq(true)
    end

    it 'returns false when comparing a closure with a method' do
      method = described_class.new(block_type: :method)

      expect(block1.type_compatible?(method)).to eq(false)
    end

    it 'returns false when compared with a void type' do
      void = Inkoc::Type::Void.new

      expect(block1.type_compatible?(void)).to eq(false)
    end

    it 'returns true when compared with an implemented trait' do
      trait = Inkoc::Type::Trait.new
      proto = Inkoc::Type::Object.new(implemented_traits: Set.new([trait]))
      block = described_class.new(prototype: proto)

      expect(block.type_compatible?(trait)).to eq(true)
    end

    it 'returns true when compared with a compatible optional type' do
      opt = Inkoc::Type::Optional.new(block2)

      expect(block1.type_compatible?(opt)).to eq(true)
    end

    it 'returns false when compared with an unimplemented trait' do
      trait = Inkoc::Type::Trait.new

      expect(block1.type_compatible?(trait)).to eq(false)
    end

    describe 'with a block with a rest argument' do
      before do
        block1.rest_argument = true
      end

      it 'returns true when the other block has a rest argument' do
        block2.rest_argument = true

        expect(block1.type_compatible?(block2)).to eq(true)
      end

      it 'returns false when the other block does not have a rest argument' do
        expect(block1.type_compatible?(block2)).to eq(false)
      end
    end

    it 'returns true when the arguments are type compatible' do
      parent = Inkoc::Type::Object.new
      child = Inkoc::Type::Object.new(prototype: parent)

      block1.define_argument('number', child)
      block2.define_argument('number', parent)

      expect(block1.type_compatible?(block2)).to eq(true)
    end

    it 'returns false when the arguments are not type compatible' do
      parent = Inkoc::Type::Object.new
      child = Inkoc::Type::Object.new

      block1.define_argument('number', child)
      block2.define_argument('number', parent)

      expect(block1.type_compatible?(block2)).to eq(false)
    end

    it 'returns true when the throw types are compatible' do
      parent = Inkoc::Type::Object.new
      child = Inkoc::Type::Object.new(prototype: parent)

      block1.throws = child
      block2.throws = parent

      expect(block1.type_compatible?(block2)).to eq(true)
    end

    it 'returns false when the throw types are not compatible' do
      parent = Inkoc::Type::Object.new
      child = Inkoc::Type::Object.new

      block1.throws = child
      block2.throws = parent

      expect(block1.type_compatible?(block2)).to eq(false)
    end

    it 'returns true when the return types are compatible' do
      parent = Inkoc::Type::Object.new
      child = Inkoc::Type::Object.new(prototype: parent)

      block1.returns = child
      block2.returns = parent

      expect(block1.type_compatible?(block2)).to eq(true)
    end

    it 'returns false when the return types are not compatible' do
      parent = Inkoc::Type::Object.new
      child = Inkoc::Type::Object.new

      block1.returns = child
      block2.returns = parent

      expect(block1.type_compatible?(block2)).to eq(false)
    end
  end

  describe '#argument_types_without_self' do
    it 'returns the argument types while ignoring the "self" argument' do
      self_type = Inkoc::Type::Object.new(name: 'A')
      name_type = Inkoc::Type::Object.new(name: 'T')

      block1.define_self_argument(self_type)
      block1.define_argument('name', name_type)

      expect(block1.argument_types_without_self).to eq([name_type])
    end
  end

  describe '#type_name' do
    before do
      block1.define_self_argument(Inkoc::Type::Object.new)
    end

    describe 'without any types defined' do
      it 'returns the type name' do
        expect(block1.type_name).to eq('do -> Dynamic')
      end
    end

    describe 'with a single argument defined' do
      it 'includes the argument type in the type name' do
        block1.define_argument('a', Inkoc::Type::Object.new(name: 'A'))

        expect(block1.type_name).to eq('do (A) -> Dynamic')
      end
    end

    describe 'with multiple arguments defined' do
      it 'includes the argument types in the type name' do
        block1.define_argument('a', Inkoc::Type::Object.new(name: 'A'))
        block1.define_argument('b', Inkoc::Type::Object.new(name: 'B'))

        expect(block1.type_name).to eq('do (A, B) -> Dynamic')
      end
    end

    describe 'with a throw type defined' do
      it 'includes the throw type in the type name' do
        block1.throws = Inkoc::Type::Object.new(name: 'A')

        expect(block1.type_name).to eq('do !! A -> Dynamic')
      end
    end

    describe 'with a custom return type defined' do
      it 'includes the return type in the type name' do
        block1.returns = Inkoc::Type::Object.new(name: 'A')

        expect(block1.type_name).to eq('do -> A')
      end
    end

    describe 'with a type parameter defined' do
      it 'includes the type parameter in the type name' do
        block1.define_type_parameter('T', Inkoc::Type::Trait.new(name: 'T'))

        expect(block1.type_name).to eq('do !(T) -> Dynamic')
      end
    end

    describe 'with multiple type parameters defined' do
      it 'includes the type parameters in the type name' do
        block1.define_type_parameter('A', Inkoc::Type::Trait.new(name: 'A'))
        block1.define_type_parameter('B', Inkoc::Type::Trait.new(name: 'B'))

        expect(block1.type_name).to eq('do !(A, B) -> Dynamic')
      end
    end

    describe 'with a block that defines everything' do
      it 'includes everything in the type name' do
        block1.define_type_parameter('T1', Inkoc::Type::Trait.new(name: 'T1'))
        block1.define_type_parameter('T2', Inkoc::Type::Trait.new(name: 'T1'))

        block1.define_argument('a', Inkoc::Type::Object.new(name: 'A'))
        block1.define_argument('b', Inkoc::Type::Object.new(name: 'B'))

        block1.throws = Inkoc::Type::Object.new(name: 'C')
        block1.returns = Inkoc::Type::Object.new(name: 'D')

        expect(block1.type_name).to eq('do !(T1, T2) (A, B) !! C -> D')
      end
    end
  end
end
